from DataStructures.data import GCODE_LOCKED_MESSAGE
from DataStructures.makesmithInitFuncs import MakesmithInitFuncs
from File.safePaths import libraryRoot, resolveInside
from Actions import firmware
from Connection.firmwareGcode import changesTool, toolNamedBy
from File.retention import keepNewest

import os
import sys
import math
import serial.tools.list_ports
import glob
import json
import time
import re
import zipfile
import subprocess
from subprocess import call


def gcodeNumbers(letter, line):
    '''
    The numbers of the letter's words in line, so M3 and M03 give 3 while M30 gives 30 and G91.1 gives 91.1.
    '''
    return {float(number) for number in re.findall(letter + r"\s*([0-9]+(?:\.[0-9]+)?)", line)}


KEEP_ARCHIVES = 5
# A restart lowers the bit at this feed when the file has not set a Z feed.
DEFAULT_PLUNGE_FEED_MM_PER_MIN = 12.8

# Manual commands that reach the controller, which runs none of them while the firmware is paused.
MACHINE_COMMANDS = {
    "move", "moveTo", "moveZ", "setMaxZ", "setMinZ", "clearZ", "getZlimits", "reportSettings", "home", "defineZ0",
    "touchZ", "moveToDefault", "returnToCenter", "macro1", "macro2",
}

HOME_AND_PID_COMMANDS = {"defineHome", "resetHomeToCenter", "executeVelocityPIDTest", "executePositionPIDTest"}
TOOL_CHANGE_WAITS_MESSAGE = (
    "Cannot issue command while a tool change waits. Press Resume or STOP before setting home or running a PID test."
)


class FirmwareUpgradeError(Exception):
    pass


'''
This class does most of the heavy lifting in processing messages from the UI client.
'''
class Actions(MakesmithInitFuncs):

    def processAction(self, msg):
        '''
        When a message comes in via the UI client it gets parsed and processed here.
        :param msg: json of message from UI client
        :return:
        '''
        try:
            if self.data.firmwarePaused and self.data.uploadFlag != 1 and msg["data"]["command"] in MACHINE_COMMANDS:
                # Streaming stays held, so releasing the firmware lets the jog run now instead of on Resume.
                self.releaseFirmwarePause()
            # Commands allowed during sending gcode.  These commands won't screw something up.
            if msg["data"]["command"] == "createDirectory":
                if not self.createDirectory(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with creating directory.")
            elif msg["data"]["command"] == "stopZ":
                if not self.stopZ():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with stopping Z-Axis movement")
            elif msg["data"]["command"] == "stopRun":
                if not self.stopRun():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with stopping run")
            elif msg["data"]["command"] == "pauseRun":
                if not self.pauseRun():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with pausing run")
            elif msg["data"]["command"] == "resumeRun":
                if not self.resumeRun():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with resuming run")
            elif msg["data"]["command"] == "toggleCamera":
                if not self.toggleCamera():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with toggling camera.")
            elif msg["data"]["command"] == "statusRequest":
                if msg["data"]["arg"] == "cameraStatus":
                    if not self.cameraStatus():
                        self.data.ui_queue1.put("Alert", "Alert", "Error with toggling camera.")
            elif msg["data"]["command"] == "queryCamera":
                if not self.queryCamera():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with querying camera.")
            elif msg["data"]["command"] == "shutdown":
                if not self.shutdown():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with shutting down.")
            elif msg["data"]["command"] == "TurnOffRPI":
                if not self.turnOff():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with system shutting down.")
            elif self.data.uploadFlag == 1:
                self.data.ui_queue1.put("Alert", "Alert", "Cannot issue command while sending gcode.")
            # Commands not allowed during sending gcode.. if you did these commands, something could screw up.
            # If uploadFlag was enabled (see above) then this would never be reached.

            elif msg["data"]["command"] == "move":
                if not self.move(msg["data"]["arg"], float(msg["data"]["arg1"])):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with initiating move.")
            elif msg["data"]["command"] == "moveTo":
                if not self.moveTo(msg["data"]["arg"], float(msg["data"]["arg1"])):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with initiating move.")
            elif msg["data"]["command"] == "moveZ":
                if not self.moveZ(msg["data"]["arg"], float(msg["data"]["arg1"])):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with initiating Z-Axis move.")
            elif msg["data"]["command"] == "setMaxZ":
                if not self.setMaxZ():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with setting Z-Axis Maximum")
            elif msg["data"]["command"] == "setMinZ":
                if not self.setMinZ():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with setting Z-Axis Minimum")
            elif msg["data"]["command"] == "clearZ":
                if not self.clearZ():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with clearing Z-Axis Min and Max")
            elif msg["data"]["command"] == "getZlimits":
                if not self.getZlimits():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with clearing Z-Axis Min and Max")
            elif msg["data"]["command"] == "reportSettings":
                self.data.gcode_queue.put("$$")
            elif msg["data"]["command"] == "home":
                if not self.home():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with returning to home.")
            elif msg["data"]["command"] == "defineZ0":
                if not self.data.actions.defineZ0():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with defining Z-Axis zero.")
            elif msg["data"]["command"] == "touchZ":
                if not self.touchZ():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with touching.")
            elif msg["data"]["command"] == "moveToDefault":
                if not self.moveToDefault():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with moving to default chain lengths")
            elif msg["data"]["command"] == "returnToCenter":
                if not self.returnToCenter():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with returning to center")
            elif msg["data"]["command"] == "moveGcodeZ":
                if not self.moveGcodeZ(int(msg["data"]["arg"])):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with moving to Z move")
            elif msg["data"]["command"] == "moveGcodeGoto":
                if not self.moveGcodeIndex(int(msg["data"]["arg"]), True):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with moving to Z move")
            elif msg["data"]["command"] == "moveGcodeIndex":
                if not self.moveGcodeIndex(int(msg["data"]["arg"])):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with moving to index")
            elif msg["data"]["command"] == "macro1":
                if not self.macro(1):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with performing macro")
            elif msg["data"]["command"] == "macro2":
                if not self.macro(2):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with performing macro")
            elif msg["data"]["command"] == "clearLogs":
                if not self.clearLogs():
                    self.data.ui_queue1.put("Alert", "Alert", "Error clearing log files.")

            elif msg["data"]["command"] == "clearGCode":
                if self.data.gcodeLocked():
                    self.data.ui_queue1.put("Alert", "Alert", GCODE_LOCKED_MESSAGE)
                elif self.clearGCode():
                    # send blank gcode to UI
                    self.data.ui_queue1.put("Action", "gcodeUpdate", "")
                else:
                    self.data.ui_queue1.put("Alert", "Alert", "Error with clearing gcode")
            # Play resumes a job the controller paused, so it passes the gate below while uploadFlag is 0.
            elif msg["data"]["command"] == "startRun" and self.data.uploadFlag == 0:
                if not self.startRun():
                    if len(self.data.gcode) > 0:
                        self.data.ui_queue1.put("Alert", "Alert", "Error with starting run.")
                    else:
                        self.data.ui_queue1.put("Alert", "Alert", "No GCode file loaded.")
            # A controller pause (M0/M1, Message:, ALARM) drops uploadFlag to 0 but holds the job like a user Pause.
            elif self.data.uploadFlag != 0 or self.data.jobInProgress:
                self.data.ui_queue1.put("Alert", "Alert", "Cannot issue command while paused sending gcode. You must press STOP before performing this action.")
            # Commands not allowed while paused.. if you did these commands, something could screw up.
            elif self.data.manualZAxisAdjust and msg["data"]["command"] in HOME_AND_PID_COMMANDS:
                self.data.ui_queue1.put("Alert", "Alert", TOOL_CHANGE_WAITS_MESSAGE)
            elif msg["data"]["command"] == "update":
                if not self.data.releaseManager.update(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with updating webcontrol.")
                return "Shutdown"
            elif msg["data"]["command"] == "cutTriangularCalibrationPattern":
                if not self.data.triangularCalibration.cutTriangularCalibrationPattern():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with cutting triangular calibration pattern.")
            elif msg["data"]["command"] == "acceptTriangularCalibrationResults":
                if not self.data.triangularCalibration.acceptTriangularCalibrationResults():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with accepting triangular calibration results.")
            elif msg["data"]["command"] == "cutHoleyCalibrationPattern":
                if not self.data.holeyCalibration.CutTestPattern():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with cutting holey calibration pattern.")
            elif msg["data"]["command"] == "acceptHoleyCalibrationResults":
                if not self.data.holeyCalibration.acceptCalibrationResults():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with accepting holey calibration results.")
            elif msg["data"]["command"] == "resetChainLengths":
                if not self.resetChainLengths():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with resetting chain lengths.")
            elif msg["data"]["command"] == "defineHome":
                posX= msg["data"]["arg"]
                posY= msg["data"]["arg1"]
                if self.defineHome(posX, posY):
                    ## the gcode file might change the active units so we need to inform the UI of the change.
                    self.data.ui_queue1.put("Action", "unitsUpdate", "")
                    self.data.ui_queue1.put("Action", "gcodeUpdate", "")
                else:
                    self.data.ui_queue1.put("Alert", "Alert", "Error with defining home.")
            elif msg["data"]["command"] == "testMotors":
                if not self.testMotors():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with testing motors")
            elif msg["data"]["command"] == "wipeEEPROM":
                if not self.wipeEEPROM(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with wiping EEPROM")
            elif msg["data"]["command"] == "setSprockets":
                if not self.setSprockets(msg["data"]["arg"], msg["data"]["arg1"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with setting sprocket")
            elif msg["data"]["command"] == "rotateSprocket":
                if not self.rotateSprocket(msg["data"]["arg"], msg["data"]["arg1"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with setting sprocket")                    
            elif msg["data"]["command"] == "setSprocketAutomatic":
                if not self.setSprocketAutomatic():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with setting sprockets automatically")
            elif msg["data"]["command"] == "setSprocketsZero":
                if not self.setSprocketsZero():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with setting sprockets zero value")
            elif msg["data"]["command"] == "setSprocketsDefault":
                if not self.setSprocketsDefault():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with setting sprockets as default")
            elif msg["data"]["command"] == "updatePorts":
                if not self.updatePorts():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with updating list of ports")
            elif msg["data"]["command"] == "optical_onStart":
                if not self.data.opticalCalibration.on_Start():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with starting optical calibration")
            elif msg["data"]["command"] == "optical_Calibrate":
                if not self.data.opticalCalibration.on_Calibrate(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with starting optical calibration")
            elif msg["data"]["command"] == "saveOpticalCalibrationConfiguration":
                if not self.data.opticalCalibration.saveOpticalCalibrationConfiguration(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with saving optical calibration configuration")
            elif msg["data"]["command"] == "stopOpticalCalibration":
                if not self.data.opticalCalibration.stopOpticalCalibration():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with stopping optical calibration.")
            elif msg["data"]["command"] == "testOpticalCalibration":
                if not self.data.opticalCalibration.testImage(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with test image.")
            elif msg["data"]["command"] == "findCenterOpticalCalibration":
                if not self.data.opticalCalibration.findCenter(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with find Center.")
            elif msg["data"]["command"] == "saveAndSendOpticalCalibration":
                if not self.data.opticalCalibration.saveAndSend():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with saving and sending calibration matrix.")
            elif msg["data"]["command"] == "reloadCalibration":
                errorX, errorY, curveX, curveY = self.data.opticalCalibration.reloadCalibration()
                if errorX is None or errorY is None or curveX is None or curveY is None:
                    self.data.ui_queue1.put("Alert", "Alert", "Error with (re)loading calibration.")
                else:
                    data = {"errorX": errorX, "errorY": errorY}
                    self.data.ui_queue1.put("Action", "updateOpticalCalibrationError", data)
            elif msg["data"]["command"] == "saveCalibrationToCSV":
                if not self.data.opticalCalibration.saveCalibrationToCSV():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with saving calibration to CSV.")
            elif msg["data"]["command"] == "clearCalibration":
                if not self.data.opticalCalibration.clearCalibration():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with clearing calibration.")
            elif msg["data"]["command"] == "curveFitOpticalCalibration":
                curveX, curveY = self.data.opticalCalibration.surfaceFit()
                #curveX, curveY = self.data.opticalCalibration.polySurfaceFit()
                if curveX is None or curveY is None:
                    self.data.ui_queue1.put("Alert", "Alert", "Error with curve fitting calibration data.")
                else:
                    data = {"curveX": curveX, "curveY": curveY}
                    self.data.ui_queue1.put("Action", "updateOpticalCalibrationCurve", data)
            elif msg["data"]["command"] == "adjustCenter":
                if not self.adjustCenter(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with adjusting center.")
            elif msg["data"]["command"] == "upgradeCustomFirmware":
                #if the upgradeFirmware process fails, it will send the Error Alert UI
                if self.upgradeFirmware(0):
                    self.data.ui_queue1.put("Alert", "Alert", "Custom firmware update complete.")
            elif msg["data"]["command"] == "upgradeStockFirmware":
                if self.upgradeFirmware(1):
                    self.data.ui_queue1.put("Alert", "Alert", "Stock firmware update complete.")
            elif msg["data"]["command"] == "upgradeHoleyFirmware":
                if self.upgradeFirmware(2):
                    self.data.ui_queue1.put("Alert", "Alert", "Custom firmware update complete.")
            elif msg["data"]["command"] == "adjustChain":
                if not self.adjustChain(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with adjusting chain.")
            elif msg["data"]["command"] == "executeVelocityPIDTest":
                if not self.velocityPIDTest(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with executing velocity PID test.")
            elif msg["data"]["command"] == "executePositionPIDTest":
                if not self.positionPIDTest(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with executing velocity PID test.")
            elif msg["data"]["command"] == "boardProcessGCode":
                if not self.data.boardManager.processGCode():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with processing gcode")
            elif msg["data"]["command"] == "boardClearBoard":
                if not self.data.boardManager.clearBoard():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with clearing board")
            elif msg["data"]["command"] == "updatePyInstaller":
                if not self.data.releaseManager.updatePyInstaller():
                    self.data.ui_queue1.put("Alert", "Alert", "Error with updating WebControl")
                return "Shutdown"
            elif msg["data"]["command"] == "setFakeServo":
                if not self.setFakeServo(msg["data"]["arg"]):
                    self.data.ui_queue1.put("Alert", "Alert", "Error with changing Fake Servo")
            elif msg["data"]["command"] == "resetHomeToCenter":
                if self.defineHome(0, 0):
                    ## notify UI of home change to request gcode update
                    self.data.ui_queue1.put("Action", "gcodeUpdate", "")
                else:
                    self.data.ui_queue1.put("Alert", "Alert", "Error with resetting home to center")
            else:
                response = "Function not currently implemented.. Sorry."
                response = response + "["+msg["data"]["command"]+"]"
                self.data.ui_queue1.put("Alert", "Alert", response)
                print(msg["data"])
        except Exception as e:
            print(str(e))


    def shutdown(self):
        '''
        If running docker, sends message to WebMCP to shutdown webcontrol.
        If running as pyinstaller, calls an exit function.
        TODO: Add option to shutdown RPI completely.
        :return:
        '''
        try:
            print(self.data.platform)
            if self.data.platform == "PYINSTALLER":
                os._exit(0)
            else:
                self.data.ui_queue1.put("WebMCP", "shutdown", "")
                return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
    
    def turnOff(self):
        ''' this is the rpi soft power button function'''
        try:
            oname = os.name
            print ("OS is ", oname)
            if oname == "posix":
                print(os.system("uname -a"))
            # insert other possible OSes here
            # ... NEED ARE YOU SURE YOU WANT POWER DOWN?
                call("sudo --non-interactive shutdown -h now", shell=True)
                return(True) #this should never run
            else:
                print("unknown OS")
                print(self.data.platform)
                #print(os.uname)
                return(False)
        except:
            print("shutdown failure")
            return(False)  
                  
    def defineHome(self, posX="", posY=""):
        '''
        Redefines the home location and sends message to update the UI client.  In a break from ground control, this
        does not alter the gcode.  Gcode is altered by the home location only when sent to the controller.
        posX and posY define the coordinates of home.  The are populated via the right-click context menu if the user
        uses the mouse to define home.  They are empty if the user presses the define home button on the frontpage
        menu, in which case the coordinates of the sled are used.
        :param posX:
        :param posY:
        :return:
        '''
        try:
            if self.data.units == "MM":
                scaleFactor = 25.4
            else:
                scaleFactor = 1.0
            # if posX and posY have values, use them, else use the sled's position.
            if posX != "" and posY != "":
                homeX = round(posX * scaleFactor, 4)
                homeY = round(posY * scaleFactor, 4)
            else:
                homeX = round(self.data.xval, 4)
                homeY = round(self.data.yval, 4)

            self.data.config.setValue("Advanced Settings", "homeX", str(homeX))
            self.data.config.setValue("Advanced Settings", "homeY", str(homeY))

            # the moveLine function of gcodeFile is still used (though its called directly by serialThread) so I still
            # track the home coordinates in gcodeShift.
            self.data.gcodeShift = [ homeX, homeY ]
            # send update to UI client with new home position
            position = {"xval": homeX, "yval": homeY}
            self.data.ui_queue1.put("Action", "homePositionMessage", position)
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def home(self):
        '''
        Directs controller to move to home location
        :return:
        '''
        try:
            self.data.gcode_queue.put("G90  ")
            safeHeightMM = float(
                self.data.config.getValue("Maslow Settings", "zAxisSafeHeight")
            )
            safeHeightInches = safeHeightMM / 25.4
            if self.data.units == "INCHES":
                self.data.gcode_queue.put("G00 Z" + "%.3f" % (safeHeightInches))
            else:
                self.data.gcode_queue.put("G00 Z" + str(safeHeightMM))
            homeX = self.data.config.getValue("Advanced Settings", "homeX")
            homeY = self.data.config.getValue("Advanced Settings", "homeY")
            self.data.gcode_queue.put(
                "G00 X"
                + str(homeX)
                + " Y"
                + str(homeY)
                + " "
            )
            self.data.gcode_queue.put("G00 Z0 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def resetChainLengths(self):
        '''
        Sends B08 command to controller to tell controller chains are at their 'extendChainLengths' distance.
        (equivalent to ground control's manualCalibrateChainLengths function)
        :return:
        '''
        try:
            self.data.gcode_queue.put("B08 ")
            # not sure why this was added here.  Probably not needed.  Todo: cleanup if not needed.
            if self.data.units == "INCHES":
                self.data.gcode_queue.put("G20 ")
            else:
                self.data.gcode_queue.put("G21 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def defineZ0(self):
        '''
        Sends G10 Z0 to controller to set current Z height as zero.
        :return:
        '''
        try:
            self.data.gcode_queue.put("G10 Z0 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def stopZ(self):
        '''
        Sends a stop signal to the controller and clears the gcode queue.  This is called from the Z-Axis popup, not
        the frontpage.  This is equivalent to ground control's stopZMove.
        :return:
        '''
        try:
            self.data.quick_queue.put("!")
            with self.data.gcode_queue.mutex:
                self.data.gcode_queue.queue.clear()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
        
    def setMinZ(self):
        '''
        Arduino command to set minimum z axis travel position to prevent z axis motor damage
        B18
        '''
        try:
            self.data.gcode_queue.put("B18")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
        
    def setMaxZ(self):
        '''
        Arduino command to set maximum z axis travel position to prevent z axis motor damage
        B17
        '''
        try:
            self.data.gcode_queue.put("B17")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
        
    def clearZ(self):
        '''
        Arduino command to clear z axis minimum and maximum
        B19
        '''
        try:
            self.data.gcode_queue.put("B19")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
        
    def getZlimits(self):
        '''
        Arduino command to echo z axis minimum and maximum values
        B20
        '''
        try:
            self.data.gcode_queue.put("B20")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
        
    def startRun(self):
        '''
        Starts the process of sending the gcode to the controller.
        :return:
        '''
        try:
            # Play while a tool change waits is a resume; a restart would lower the new bit to the old tool's Z.
            if self.data.manualZAxisAdjust or self.data.jobInProgress:
                return self.resumeRun()
            if len(self.data.gcode) > 0:
                # set current Z target to the current z height in case gcode doesn't include a z move before an xy move.
                # if it doesn't and the user pauses during an xy move, then the target Z is set to 0.  This sets it to
                # what it currently is when the user started the gcode send.
                self.data.currentZTarget = self.data.zval
                # if the gcode index is not 0, then make sure the machine is in the proper state before starting to send
                # the gcode.
                if self.data.gcodeIndex > 0:
                    # get machine into proper state by sending appropriate commands
                    self.processGCode()
                    # update the gcode position on the UI client.. Have to recalculate it from the gcode because
                    # starting at some place other than 0
                    self.sendGCodePositionUpdate(recalculate=True)
                    self.data.uploadFlag = 1
                else:
                    self.data.uploadFlag = 1
                self.data.jobInProgress = True
                self.data.gpioActions.causeAction("PlayLED", "on")
                return True
            else:
                return False
        except Exception as e:
            # something goes wrong, stop uploading.
            self.data.console_queue.put(str(e))
            self.data.uploadFlag = 0
            self.data.jobInProgress = False
            self.data.gcodeIndex = 0
            return False

    def stopRun(self):
        '''
        Stops the uploading of gcode.
        :return:
        '''
        try:
            self.data.console_queue.put("stopping run")
            # this is necessary because of the way PID data is being processed.  Otherwise could potentially get stuck
            # in PID test
            self.data.inPIDPositionTest = False
            self.data.inPIDVelocityTest = False
            self.data.uploadFlag = 0
            self.data.jobInProgress = False
            self.data.gcodeIndex = 0
            self.data.runGeneration += 1
            self.data.manualZAxisAdjust = False
            self.data.firmwarePaused = False
            self.data.quick_queue.put("!")
            with self.data.gcode_queue.mutex:
                self.data.gcode_queue.queue.clear()
            # TODO: app.onUploadFlagChange(self.stopRun, 0) edit: not sure this is needed anymore
            self.data.console_queue.put("Gcode stopped")
            self.sendGCodePositionUpdate(self.data.gcodeIndex)
            # notify UI client to clear any alarm that's active because a stop has been process.
            self.data.ui_queue1.put("Action", "clearAlarm", "")
            self.data.gpioActions.causeAction("StopLED", "on")
            # reset pause
            self.data.ui_queue1.put("Action", "setAsPause", "")
            self.data.gpioActions.causeAction("PauseLED", "off")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def moveToDefault(self):
        '''
        Moves the sled to the spot where the chains are extended to the extendChainLength.  Not regularly used, but
        more for testing.  Uses B09 commands (single axis moves).
        :return:
        '''
        try:
            chainLength = self.data.config.getValue(
                "Advanced Settings", "chainExtendLength"
            )
            self.data.gcode_queue.put("G90 ")
            self.data.gcode_queue.put(
                "B09 R" + str(chainLength) + " L" + str(chainLength) + " "
            )
            self.data.gcode_queue.put("G91 ")

            # the firmware doesn't update sys.xPosition or sys.yPosition during a singleAxisMove
            # therefore, the machine doesn't know where it is.
            # so tell the machine where it's at by sending a reset chain length
            self.data.gcode_queue.put("B08 ")

            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def testMotors(self):
        '''
        Sends the test motors/encoder command, B04
        :return:
        '''
        try:
            self.data.gcode_queue.put("B04 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def wipeEEPROM(self, extent):
        '''
        Sends the wipe EEPROM commands
        :param extent:
        :return:
        '''
        try:
            if extent == "All":
                self.data.gcode_queue.put("$RST=* ")
            elif extent == "Settings":
                self.data.gcode_queue.put("$RST=$ ")
            elif extent == "Maslow":
                self.data.gcode_queue.put("$RST=# ")
            else:
                return False

            #sync settings after 2 seconds (give time form controller to reset)
            time.sleep(2)
            self.data.gcode_queue.put("$$")

            #reset chain lengths so they aren't zero
            if extent == "All" or extent == "Maslow":
                self.resetChainLengths()

            # these two lines were commented out and aren't used (and may not even work).  The thought was that after
            # the EEPROM got wiped, you need to sync settings.
            #self.timer = threading.Timer(6.0, self.data.gcode_queue.put("$$"))
            #self.timer.start()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def pauseRun(self):
        '''
        Pause the current uploading of gcode.  Notify UI client to change the Pause button to say Resume
        :return:
        '''
        try:
            if self.data.uploadFlag == 1:
                self.data.uploadFlag = 2
                self.data.console_queue.put("Run Paused")
                self.data.ui_queue1.put("Action", "setAsResume", "")
                # The idea was to be able to make sure the machine returns to
                # the correct z-height after a pause in the event the user raised/lowered the bit.
                #self.data.pausedzval = self.data.zval
                #self.data.pausedUnits = self.data.units
                self.data.pausedzval = self.data.currentZTarget
                self.data.pausedUnits = self.data.units
                self.data.pausedPositioningMode = self.data.positioningMode
                self.data.pausedGcodeIndex = self.data.gcodeIndex
                #print("Saving paused positioning mode: " + str(self.data.pausedPositioningMode))
                self.data.gpioActions.causeAction("PauseLED", "on")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def resumeRun(self):
        '''
        Resumes sending the gcode.  If a tool change command was received, then the manualZAxisAdjust is enabled and
        the machine will be returned to the z-axis height it was when the tool change command was processed.  It also
        makes sure the units are what they were prior to the tool change (user might have switched them up while
        setting z-axis to zero).
        :return:
        '''
        try:
            # Restore self.data.upladFlag properly
            if self.data.manualZAxisAdjust:
                # Z-axis is disabled and requires manual adjustment.
                print("Resume run with manual z-axis adjust.")
                self.data.manualZAxisAdjust = False
                self.data.uploadFlag = 1 if self.data.jobInProgress else 0
            elif self.data.uploadFlag == 1:
                # A second Resume (double click, another client) must not lift and replunge mid-cut.
                return True
            elif self.data.uploadFlag == 0 and not self.data.jobInProgress:
                # An M0 sent from the console paused the firmware with no job to carry on.
                pass
            else:
                # The user may have jogged, homed or moved the gcode index while paused, so return to the file's
                # position at the current index at safe height before lowering the bit.
                print("Resume run without manual z-axis adjust.")
                if self.data.pausedUnits is not None and self.data.pausedUnits != self.data.units:
                    print("Restoring units to:" + str(self.data.pausedUnits))
                    self.updateSetting("toInches" if self.data.pausedUnits == "INCHES" else "toMM", 0, True)
                zTarget = None
                # currentZTarget is the last Z sent, which only matches the file's Z if the index has not moved.
                if self.data.pausedGcodeIndex == self.data.gcodeIndex and self.data.pausedzval is not None:
                    zTarget = self.data.pausedzval
                self.queueRestartSequence(self.gcodeStateAt(self.data.gcodeIndex), zTarget, changeTool=False)
                self.data.pausedUnits = None
                self.data.pausedzval = None
                self.data.pausedPositioningMode = None
                self.data.pausedGcodeIndex = None

                self.sendGCodePositionUpdate(self.data.gcodeIndex, recalculate=True)
                self.data.uploadFlag = 1

            # Harmless when the firmware is not paused: it just answers ok.
            self.releaseFirmwarePause()
            self.data.ui_queue1.put("Action", "setAsPause", "")
            self.data.gpioActions.causeAction("PauseLED", "off")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def releaseFirmwarePause(self):
        self.data.firmwarePaused = False
        self.data.quick_queue.put("~")

    def returnToCenter(self):
        '''
        Instructs controller to move sled to 0,0 at safe height.
        :return:
        '''
        try:
            self.data.gcode_queue.put("G90  ")
            safeHeightMM = float(
                self.data.config.getValue("Maslow Settings", "zAxisSafeHeight")
            )
            safeHeightInches = safeHeightMM / 25.4
            if self.data.units == "INCHES":
                self.data.gcode_queue.put("G00 Z" + "%.3f" % (safeHeightInches))
            else:
                self.data.gcode_queue.put("G00 Z" + str(safeHeightMM))
            self.data.gcode_queue.put("G00 X0.0 Y0.0 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def clearGCode(self):
        '''
        Calls function to clear the gcode file.  Probably could have just been called directly from processAction
        :return:
        '''
        try:
            self.data.gcodeFile.clearGcodeFile()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def moveGcodeZ(self, moves):
        '''
        Moves the gcode index to the next z-axis move.
        :param moves:
        :return:
        '''
        try:
            dist = 0
            #determine the number of lines to move to reach the next Z-axis move.
            for index, zMove in enumerate(self.data.zMoves):
                if moves > 0 and zMove > self.data.gcodeIndex:
                    dist = self.data.zMoves[index + moves - 1] - self.data.gcodeIndex
                    break
                if moves < 0 and zMove < self.data.gcodeIndex:
                    dist = self.data.zMoves[index + moves + 1] - self.data.gcodeIndex
            if self.moveGcodeIndex(dist):
                # this command will continue on in the moveGcodeIndex "if"
                return True
            else:
                return False
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def moveGcodeIndex(self, dist, index=False):
        '''
        Moves the gcodeIndex by either the distance or, in index is True, uses the dist as the index.
        :param dist:
        :param index:
        :return:
        '''
        try:
            maxIndex = len(self.data.gcode) - 1
            if index is True:
                targetIndex = dist
            else:
                targetIndex = self.data.gcodeIndex + dist

            # check to see if we are still within the length of the file
            if maxIndex < 0:  # break if there is no data to read
                return
            elif targetIndex < 0:  # negative index not allowed
                self.data.gcodeIndex = 0
            elif (
                targetIndex > maxIndex
            ):  # reading past the end of the file is not allowed
                self.data.gcodeIndex = maxIndex
            else:
                self.data.gcodeIndex = targetIndex

            try:
                # send gcode position update to UI client
                retval = self.sendGCodePositionUpdate(recalculate=True)
                return retval
            except Exception as e:
                self.data.console_queue.put(str(e))
                self.data.console_queue.put("Unable to update position for new gcode line")
                return False
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def move(self, direction, distToMove):
        '''
        Issues commands to move the sled
        :param direction: direction to move
        :param distToMove: distance to move
        :return:
        '''
        # if enabled, for diagonal movements, the x and y move distance will be calculated such that the total distance
        # moved equals what was specified.  For example, if enabled and command is issued to move one foot to top right,
        # then the x and y coordinates will be calculated as the 0.707 feet so that a total of 1 foot is moved.  If
        # disabled, then the sled will move 1 foot left and 1 foot up, for a total distance of 1.414 feet.
        if self.data.config.getValue("WebControl Settings","diagonalMove") == 1:
            diagMove = round(math.sqrt(distToMove*distToMove/2.0), 4)
        else:
            diagMove = distToMove
        try:
            self.data.gcode_queue.put("G91 ")
            if direction == "upLeft":
                self.data.gcode_queue.put("G00 X"+ str(-1.0 * diagMove)+ " Y"+ str(diagMove)+ " ")
            elif direction == "up":
                self.data.gcode_queue.put("G00 Y" + str(distToMove) + " ")
            elif direction == "upRight":
                self.data.gcode_queue.put("G00 X" + str(diagMove) + " Y" + str(diagMove) + " ")
            elif direction == "left":
                self.data.gcode_queue.put("G00 X" + str(-1.0 * distToMove) + " ")
            elif direction == "right":
                self.data.gcode_queue.put("G00 X" + str(distToMove) + " ")
            elif direction == "downLeft":
                self.data.gcode_queue.put("G00 X" + str(-1.0 * diagMove) + " Y" + str(-1.0 * diagMove) + " ")
            elif direction == "down":
                self.data.gcode_queue.put("G00 Y" + str(-1.0 * distToMove) + " ")
            elif direction == "downRight":
                self.data.gcode_queue.put("G00 X" + str(diagMove) + " Y" + str(-1.0 * diagMove) + " ")
            else:
                return False
            self.data.gcode_queue.put("G90 ")
            # keep track of the distToMove value
            self.data.config.setValue("Computed Settings", "distToMove", distToMove)
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def moveZ(self, direction, distToMoveZ):
        '''
        Moves the Z-Axis a distance and direction
        :param direction:
        :param distToMoveZ:
        :return:
        '''

        try:
            #keep track of distToMoveZ value
            self.data.config.setValue("Computed Settings", "distToMoveZ", distToMoveZ)
            # It's possible the front page is set for one units and when you go to z-axis control, you might switch
            # to a different unit.  Webcontrol keeps these units separate, so we need to make sure the machine is in
            # the correct units when the z-axis move is sent
            unitsZ = self.data.config.getValue("Computed Settings", "unitsZ")
            previousUnits = self.data.config.getValue("Computed Settings", "units")
            if unitsZ == "MM":
                self.data.gcode_queue.put("G21 ")
            else:
                self.data.gcode_queue.put("G20 ")
            if direction == "raise":
                self.data.gcode_queue.put("G91 ")
                self.data.gcode_queue.put("G00 Z" + str(float(distToMoveZ)) + " ")
                self.data.gcode_queue.put("G90 ")
            elif direction == "lower":
                self.data.gcode_queue.put("G91 ")
                self.data.gcode_queue.put("G00 Z" + str(-1.0 * float(distToMoveZ)) + " " )
                self.data.gcode_queue.put("G90 ")
            # now, since we might have changed the units of the machine, make sure they are set back to what it was
            # originally.
            #units = self.data.config.getValue("Computed Settings", "units")
            if previousUnits != unitsZ:
                if previousUnits == "MM":
                    self.data.gcode_queue.put("G21 ")
                else:
                    self.data.gcode_queue.put("G20 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False


    def touchZ(self):
        '''
        Sends a gcode line to set z axis depth using touch plate.  I've not personally tested this.
        :return:
        '''
        try:
            plungeDepth = self.data.config.getValue("Advanced Settings", "maxTouchProbePlungeDistance")
            revertToInches = False
            if self.data.units == "INCHES":
                revertToInches = True
                self.data.gcode_queue.put("G21")
            # The firmware probes at a fixed 60 mm/min; an F here would only become the job's modal feed.
            self.data.gcode_queue.put("G90 G38.2 Z-" + plungeDepth + " M02")
            if revertToInches:
                self.data.gcode_queue.put("G20")
            # don't think this line is needed
            # todo: remove if not needed.
            self.data.measureRequest = self.defineZ0()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def updateSetting(self, setting, value, fromGcode = False):
        '''
        update settings that come from frontpage or from the gcode file.
        :param setting:
        :param value:
        :param fromGcode:
        :return:
        '''
        try:
            self.data.console_queue.put("at update setting from gcode("+str(fromGcode)+"): "+setting+" with value: "+str(value))
            # if front page button has been pressed or serialPortThread is going to send gcode with a G21 or G20..
            if setting == "toInches" or setting == "toMM":
                # this shouldn't be reached any more after I reordered the processActions function
                # todo: remove this if if not needed.
                if self.data.uploadFlag == 1 and fromGcode == False:
                    self.data.ui_queue1.put("Alert", "Alert", "Cannot change units while sending gcode.")
                    return True
                scaleFactor = 0
                if fromGcode:
                    # if from a gcode line, then update the get the distToMove value from storage and figure out the
                    # scaleFactor
                    value = float(self.data.config.getValue("Computed Settings", "distToMove"))
                    if self.data.units == "INCHES":
                        if setting == "toMM":
                            value = value * 25.4
                            scaleFactor = 25.4
                        else:
                            scaleFactor = 1.0
                    if self.data.units == "MM":
                        if setting == "toInches":
                            value = value / 25.4
                            scaleFactor = 1.0 / 25.4
                        else:
                            scaleFactor = 1.0

                if setting == "toInches":
                    # was metric, now going to imperial
                    self.data.units = "INCHES"
                    self.data.config.setValue("Computed Settings", "units", self.data.units)
                    # if scaleFactor == 0, then it wasn't previously set by the if fromGcode.
                    if scaleFactor == 0:
                        scaleFactor = 1.0/25.4
                    self.data.tolerance = 0.020
                    self.data.config.setValue("Computed Settings", "tolerance", self.data.tolerance)
                    # only send G20 to the controller if not already sending G20
                    if not fromGcode:
                        self.data.gcode_queue.put("G20 ")
                else:
                    # was imperial, now going to metric
                    self.data.units = "MM"
                    self.data.config.setValue("Computed Settings", "units", self.data.units)
                    # if scaleFactor == 0, then it wasn't previously set by the if fromGcode.
                    if scaleFactor == 0:
                        scaleFactor = 25.4
                    self.data.tolerance = 0.5
                    self.data.config.setValue("Computed Settings", "tolerance", self.data.tolerance)
                    # only send G21 to the controller if not already sending G21
                    if not fromGcode:
                        self.data.gcode_queue.put("G21 ")
                # update the gcodeShift coordinates to match units
                self.data.gcodeShift = [
                  self.data.gcodeShift[0] * scaleFactor,
                  self.data.gcodeShift[1] * scaleFactor,
                ]
                self.data.feedRate = scaleFactor*self.data.feedRate
                # The UI client sending this has already converted the distToMove to the correct value for the units
                self.data.config.setValue("Computed Settings", "distToMove", value)
                # get the current homeX and homeY coordinates
                oldHomeX = float(self.data.config.getValue("Advanced Settings", "homeX"))
                oldHomeY = float(self.data.config.getValue("Advanced Settings", "homeY"))
                # convert homeX and homeY to the correct units and save
                self.data.config.setValue("Advanced Settings", "homeX", oldHomeX * scaleFactor)
                self.data.config.setValue("Advanced Settings", "homeY", oldHomeY * scaleFactor)
                # notify all UI clients that units have been changed and distToMove has been changed
                self.data.ui_queue1.put("Action", "unitsUpdate", "")
                self.data.ui_queue1.put("Action", "distToMoveUpdate", "")
                # update the home position..
                position = {"xval": oldHomeX * scaleFactor, "yval": oldHomeY * scaleFactor}
                self.data.ui_queue1.put("Action", "homePositionMessage", position)
                # update the gcode position
                self.sendGCodePositionUpdate(recalculate=True)
            elif setting == "toInchesZ":
                # this shouldn't be reached any more after I reordered the processActions function
                # todo: remove this if if not needed.
                if self.data.uploadFlag == 1:
                    self.data.ui_queue1.put("Alert", "Alert", "Cannot change units while sending gcode.")
                    return True

                #self.data.units = "INCHES" ### commented this out and added the unitZ
                self.data.config.setValue(
                    "Computed Settings", "unitsZ", "INCHES"
                )
                self.data.config.setValue("Computed Settings", "distToMoveZ", value)
                self.data.ui_queue1.put("Action", "unitsUpdateZ", "")
                self.data.ui_queue1.put("Action", "distToMoveUpdateZ", "")

            elif setting == "toMMZ":
                # this shouldn't be reached any more after I reordered the processActions function
                # todo: remove this if if not needed.
                if self.data.uploadFlag == 1:
                    self.data.ui_queue1.put("Alert", "Alert", "Cannot change units while sending gcode.")
                    return True
                #self.data.units = "MM" ### commented this out
                self.data.config.setValue(
                    "Computed Settings", "unitsZ", "MM"
                )
                self.data.config.setValue("Computed Settings", "distToMoveZ", value)
                self.data.ui_queue1.put("Action", "unitsUpdateZ", "")
                self.data.ui_queue1.put("Action", "distToMoveUpdateZ", "")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def rotateSprocket(self, sprocket, time):
        '''
        Turns sprocket at 100 speed for given amount of time using B11 command
        This command is used in PID tuning to try to get sprockets in a position without using encoder.
        :param sprocket:
        :param time:
        :return:
        '''
        try:
            if  time > 0:
                self.data.gcode_queue.put("B11 "+sprocket+" S100 T"+str(time))
            else:
                self.data.gcode_queue.put("B11 "+sprocket+" S-100 T"+str(abs(time)))
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

            
    def setSprockets(self, sprocket, degrees):
        '''
        Moves sprocket a specified number of degrees.
        :param sprocket:
        :param degrees:
        :return:
        '''
        try:
            # calculate the amoount of chain to be fed out/in based upon sprocket circumference and degree
            degValue = round(
                float(self.data.config.getValue("Advanced Settings", "gearTeeth"))
                * float(self.data.config.getValue("Advanced Settings", "chainPitch"))
                / 360.0
                * degrees,
                4,
            )
            # Adjust distance based upon chainOverSprocket value
            self.data.gcode_queue.put("G91 ")
            if (
                self.data.config.getValue("Advanced Settings", "chainOverSprocket")
                == "Bottom"
            ):
                degValue *= -1.0
            if self.data.config.getValue("Advanced Settings", "chainOverSprocket") == "Bottom":
                if self.data.config.getValue("Computed Settings", "chainOverSprocketComputed") != 2:
                   self.data.ui_queue1.put("Alert", "Alert", "mismatch between setting and computed setting. set for bottom feed, but computed !=2. report as issue on github please")
            if self.data.config.getValue("Advanced Settings", "chainOverSprocket") == "Top":
                if self.data.config.getValue("Computed Settings", "chainOverSprocketComputed") != 1:
                   self.data.ui_queue1.put("Alert", "Alert", "mismatch between setting and computed setting. set for top feed, but computed != 1. report as issue on github please")
            # send command
            self.data.gcode_queue.put("B09 " + sprocket + str(degValue) + " ")
            self.data.gcode_queue.put("G90 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def setSprocketAutomatic(self):
        '''
        Sets the call back for the automatic sprocket alignment and requests the measurement.
        '''
        try:
            self.data.measureRequest = self.getLeftChainLength
            # request a measurement
            self.data.gcode_queue.put("B10 L")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def getLeftChainLength(self, dist):
        '''
        Part of setSprocketAutomatic.  Is called after the measurement is received.  Left chain measurement is logged
        and then callback is set to get the right chain measurement.
        :param dist:
        :return:
        '''
        self.leftChainLength = dist
        # set the call back for the measurement
        self.data.measureRequest = self.getRightChainLength
        # request a measurement
        self.data.gcode_queue.put("B10 R")

    def getRightChainLength(self, dist):
        '''
        Part of setSprocketAutomatic.  Is called after the measurement is received.  Right chain measurement is logged
        and then moveToVertical is called.
        :param dist:
        :return:
        '''
        self.rightChainLength = dist
        self.moveToVertical()

    def moveToVertical(self):
        '''
        Using distances logged from previous calls, calculates how much to move the sprockets so that one tooth is
        vertical.
        :return:
        '''
        chainPitch = float(self.data.config.getValue("Advanced Settings", "chainPitch"))
        gearTeeth = float(self.data.config.getValue("Advanced Settings", "gearTeeth"))
        distPerRotation = chainPitch * gearTeeth

        distL = -1 * (self.leftChainLength % distPerRotation)
        distR = -1 * (self.rightChainLength % distPerRotation)

        # Issue required move commands..
        self.data.gcode_queue.put("G91 ")
        self.data.gcode_queue.put("B09 L" + str(distL) + " ")
        self.data.gcode_queue.put("B09 R" + str(distR) + " ")
        self.data.gcode_queue.put("G90 ")

    def setSprocketsZero(self):
        '''
        Notify controller that the chain length is zero.  This is called by the user after the sprockets are set to
        vertical.
        :return:
        '''
        try:
            self.data.gcode_queue.put("B06 L0 R0 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def setSprocketsDefault(self):
        '''
        Notify controller that the chain length is equal to the extendChainLength distance.  This is called by the user
        after setting both sprockets to vertical and reapplying the appropriate chain links to the top teeth.
        https://forums.maslowcnc.com/t/how-to-manually-reset-chains-the-easy-way/
        :return:
        '''
        try:
            self.data.gcode_queue.put("B08 ")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def updatePorts(self):
        '''
        Updates the list of ports found on the computer.
        :return:
        '''
        self.data.console_queue.put("at Update Ports")
        portsList = []
        try:
            if sys.platform.startswith("linux") or sys.platform.startswith("cygwin"):
                # this excludes your current terminal "/dev/tty"
                sysports = glob.glob("/dev/tty[A-Za-z]*")
                for port in sysports:
                    portsList.append(port)
            elif sys.platform.startswith("darwin"):
                sysports = glob.glob("/dev/tty.*")
                for port in sysports:
                    portsList.append(port)
            elif sys.platform.startswith("win"):
                list = serial.tools.list_ports.comports()
                for element in list:
                    portsList.append(element.device)
            if len(portsList) == 0:
                portsList.append("None")
            self.data.comPorts = portsList
            # send updated list to UI client
            self.data.ui_queue1.put("Action", "updatePorts", "")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def acceptTriangularKinematicsResults(self):
        '''
        Calls function to accept the results of the triangular calibration.
        Todo: call directly from processActions.
        :return:
        '''
        try:
            self.data.triangularCalibration.acceptTriangularKinematicsResults()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

            
    def calibrate(self, result):
        '''
        Sends form data for triangular calibration to the triangular calibration routine to calculate and then return
        results to the UI client
        :param result:
        :return: (motorYoffset, rotationRadius, chainSagCorrection, cut34Yoffset), or None if it failed
        '''
        try:
            return self.data.triangularCalibration.calculate(result)
        except Exception as e:
            self.data.console_queue.put(str(e))
            return None

    def holeyCalibrate(self, result):
        '''
        Sends form data for holey calibration to the holey calibration routine to calculate and then return
        results to the UI client
        :param result:
        :return: (motorYoffset, distanceBetweenMotors, leftChainTolerance, rightChainTolerance, error), or None
        '''
        try:
            return self.data.holeyCalibration.Calibrate(result)
        except Exception as e:
            self.data.console_queue.put(str(e))
            return None


    def sendGCode(self, gcode):
        '''
        Sends gcode entered to controller.  Comes from the Gcode->Send GCode function
        :param gcode:
        :return:
        '''
        try:
            self.data.sentCustomGCode = gcode
            gcodeLines = gcode.splitlines()
            for line in gcodeLines:
                self.data.gcode_queue.put(line)
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
    
    def macro(self, number):
        '''
        Sends gcode associated with macro buttons
        :param number:
        :return:
        '''
        '''
        Ignore this stuff in this comment block.. used for testing gpio
        try:
            if number == 1:
                print("here")
                btn_pin = Device.pin_factory.pin(2)
                btn_pin.drive_low()
                return True
            if number == 2:
                print("here2")
                self.data.gpioActions.setGPIOAction(3, "Stop")
                return True

        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

        '''
        try:
            if number == 1:
                macro = self.data.config.getValue("Maslow Settings", "macro1")
            else:
                macro = self.data.config.getValue("Maslow Settings", "macro2")
            self.data.gcode_queue.put(macro)
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def testImage(self):
        '''
        Calls function to send the test image from optical calibration
        Todo: move to processAction
        :return:
        '''
        try:
            self.data.opticalCalibration.testImage()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def adjustCenter(self, dist):
        '''
        Used in optical calibration to allow user to raise/lower the center point and then move there.
        :param dist:
        :return:
        '''
        try:
            motorOffsetY = float(self.data.config.getValue("Maslow Settings","motorOffsetY"))+dist
            self.data.config.setValue('Maslow Settings', 'motorOffsetY', str(motorOffsetY))
            if not self.returnToCenter():
                return False
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def processSettingRequest(self, section, setting):
        '''
        Returns requested settings to the UI clients.  Needed when a client connects or reconnects to get in sync
        :param section:
        :param setting:
        :return:
        '''
        try:
            if setting == "homePosition":
                # send home position
                homeX = self.data.config.getValue("Advanced Settings", "homeX")
                homeY = self.data.config.getValue("Advanced Settings", "homeY")
                position = {"xval": homeX, "yval": homeY}
                self.data.ui_queue1.put("Action", "homePositionMessage", position)
                return None, None
            elif setting == "calibrationCurve":
                # send optical calibration curve values
                try:
                    xCurve = [0,0,0,0,0,0]
                    yCurve = [0,0,0,0,0,0]
                    xCurve[0] = float(self.data.config.getValue('Optical Calibration Settings', 'calX0'))
                    xCurve[1] = float(self.data.config.getValue('Optical Calibration Settings', 'calX1'))
                    xCurve[2] = float(self.data.config.getValue('Optical Calibration Settings', 'calX2'))
                    xCurve[3] = float(self.data.config.getValue('Optical Calibration Settings', 'calX3'))
                    xCurve[4] = float(self.data.config.getValue('Optical Calibration Settings', 'calX4'))
                    xCurve[5] = float(self.data.config.getValue('Optical Calibration Settings', 'calX5'))
                    yCurve[0] = float(self.data.config.getValue('Optical Calibration Settings', 'calY0'))
                    yCurve[1] = float(self.data.config.getValue('Optical Calibration Settings', 'calY1'))
                    yCurve[2] = float(self.data.config.getValue('Optical Calibration Settings', 'calY2'))
                    yCurve[3] = float(self.data.config.getValue('Optical Calibration Settings', 'calY3'))
                    yCurve[4] = float(self.data.config.getValue('Optical Calibration Settings', 'calY4'))
                    yCurve[5] = float(self.data.config.getValue('Optical Calibration Settings', 'calY5'))
                    data = {"curveX": xCurve, "curveY": yCurve}
                    self.data.ui_queue1.put("Action", "updateOpticalCalibrationCurve", data)
                except Exception as e:
                    self.data.console_queue.put(str(e))
            elif setting == "calibrationError":
                # send calibration error matrix
                try:
                    xyErrorArray = self.data.config.getValue("Optical Calibration Settings","xyErrorArray")
                    # parse the error array because its stored as a long string
                    errorX, errorY = self.data.config.parseErrorArray(xyErrorArray, True)
                    data = {"errorX": errorX, "errorY": errorY}
                    self.data.ui_queue1.put("Action", "updateOpticalCalibrationError", data)
                except Exception as e:
                    self.data.console_queue.put(str(e))
            elif setting == "pauseButtonSetting":
                # send current pause button state
                try:
                    if self.data.uploadFlag == 0 or self.data.uploadFlag == 1:
                        return setting, "Pause"
                    else:
                        return setting, "Resume"
                except Exception as e:
                    self.data.console_queue.put(str(e))
            else:
                # send whatever is being request
                if setting == "units":
                    # sled position messages from controller aren't sent unless there's a change, so we set the
                    # xval_prev to some silly number and when the next position message comes in from the controller,
                    # it will be processed and passed on to the UI clients.
                    self.data.xval_prev = -99999.0
                retval = self.data.config.getValue(section, setting)
                # return the setting requested and its value.
                return setting, retval
        except Exception as e:
            pass
        return None, None

    def upgradeFirmware(self, version):
        '''
        Flashes the bundled firmware (0 custom, 1 stock, 2 holey) to the controller, alerting on any failure.
        '''
        home = firmware.installRoot(self.data)
        port = str(self.data.comport)
        closeRequested = False
        try:
            if not firmware.isSerialPortName(port, self.data.comPorts):
                raise FirmwareUpgradeError("Cannot upgrade firmware: '" + port + "' is not a serial port.")
            variant = firmware.UPGRADE_VARIANTS[version]
            self.data.ui_queue1.put(
                "SpinnerMessage", "", variant.capitalize() + " Firmware Update in Progress, Please Wait."
            )
            hexFiles = firmware.firmwareFiles(home, variant)
            if not hexFiles:
                raise FirmwareUpgradeError("No firmware file found in " + firmware.firmwareFolder(home, variant))
            wasConnected = self.data.connectionStatus
            closeRequested = True
            self.data.serialPort.closeConnection()
            if wasConnected and not self.waitForSerialClose(firmware.PORT_CLOSE_SECONDS):
                raise FirmwareUpgradeError("The serial port did not close, so the firmware was not loaded.")
            # Any reconnect attempt while avrdude runs sees this and closes the port straight away.
            self.data.serialPort.serialPortRequest = "requestToClose"
            time.sleep(1.5)
            x = subprocess.run(
                firmware.avrdudeCommand(home, port, hexFiles[-1]), stderr=subprocess.STDOUT, stdout=subprocess.PIPE
            )
            if x.returncode != 0:
                raise FirmwareUpgradeError("Error loading firmware: " + str(x.stdout, "utf-8", "replace"))
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            self.data.ui_queue1.put("Alert", "Alert", str(e))
            return False
        finally:
            if closeRequested:
                self.data.serialPort.serialPortRequest = ""
                self.data.connectionStatus = 0
            self.data.ui_queue1.put("Action", "closeModals", "Notification:")

    def waitForSerialClose(self, timeout):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.data.serialPort.getConnectionStatus() == "Closed":
                return True
            time.sleep(0.05)
        return self.data.serialPort.getConnectionStatus() == "Closed"


    def createDirectory(self, _directory):
        '''
        Called to create a directory when saving gcode
        :param _directory:
        :return:
        '''
        try:
            directory = resolveInside(libraryRoot(self.data.config, "gcode"), _directory)
            if not os.path.isdir(directory):
                os.mkdir(directory)
            data = {"directory": _directory}
            self.data.ui_queue1.put("Action", "updateDirectories", data)
        except Exception as e:
            print(e)
        return True

    def sendGCodePositionUpdate(self, gCodeLineIndex=None, recalculate=False):
        '''
        Send the updated gcode position.
        :param gCodeLineIndex: Specific gcode index to send position based upon
        :param recalculate: recalculate position by parsing through the gcode file.
        :return:
        '''
        if self.data.gcode:
            if gCodeLineIndex is None:
                gCodeLineIndex = self.data.gcodeIndex
            gCodeLine = self.data.gcode[gCodeLineIndex]

            if not recalculate and gCodeLine.find("(") == -1 and gCodeLine.find(";") == -1:
                # parse the position from the gcode line or use previous x, y, or z position if not present.  Works only
                # if in absolute mode
                x = re.search("X(?=.)([+-]?([0-9]*)(\.([0-9]+))?)", gCodeLine)
                if x:
                    if self.data.positioningMode == 0:
                        xTarget = float(x.groups()[0])
                    else:
                        xTarget = float(x.groups()[0]) + self.data.previousPosX
                    self.data.previousPosX = xTarget
                else:
                    xTarget = self.data.previousPosX

                y = re.search("Y(?=.)([+-]?([0-9]*)(\.([0-9]+))?)", gCodeLine)

                if y:
                    if self.data.positioningMode == 0:
                        yTarget = float(y.groups()[0])
                    else:
                        yTarget = float(y.groups()[0]) + self.data.previousPosY
                    self.data.previousPosY = yTarget
                else:
                    yTarget = self.data.previousPosY

                z = re.search("Z(?=.)([+-]?([0-9]*)(\.([0-9]+))?)", gCodeLine)

                if z:
                    if self.data.positioningMode == 0:
                        zTarget = float(z.groups()[0])
                    else:
                        zTarget = float(z.groups()[0]) + self.data.previousPosZ
                    self.data.previousPosZ = zTarget
                else:
                    zTarget = self.data.previousPosZ

                f = re.search("F(?=.)([+-]?([0-9]*)(\.([0-9]+))?)", gCodeLine)
                if f:
                    self.data.feedRate =float(f.groups()[0])
            else:
                # if directed to recalculate or there's a comment in the line, then use more time consuming method.
                xTarget, yTarget, zTarget = self.findPositionAt(self.data.gcodeIndex)

            # correct for scaling.  If the gcode line is in one units but machine is in different units, then need
            # to make adjustment.  This might occur if a gcode is loaded that's metric and then the user switches
            # the front page to imperial and then changes the gcode index.
            scaleFactor = 1.0
            if self.data.gcodeFileUnits == "MM" and self.data.units == "INCHES":
                scaleFactor = 1/25.4
            if self.data.gcodeFileUnits == "INCHES" and self.data.units == "MM":
                scaleFactor = 25.4

            # send the position to the UI client
            position = {"xval": xTarget * scaleFactor, "yval": yTarget * scaleFactor,"zval": zTarget * scaleFactor, "gcodeLine": gCodeLine, "gcodeLineIndex": gCodeLineIndex}
            self.data.ui_queue1.put("Action", "gcodePositionMessage", position)
            return True

    def moveTo(self, posX, posY):
        '''
        Commands controller to move sled to specified coordinates from the contextmenu moveto command(always in inches)
        :param posX:
        :param posY:
        :return:
        '''
        # make sure not out of range.
        bedHeight = float(self.data.config.getValue("Maslow Settings","bedHeight"))/25.4
        bedWidth = float(self.data.config.getValue("Maslow Settings", "bedWidth"))/25.4
        try:
            if posX<=bedWidth/2 and posX>=bedWidth/-2 and posY<=bedHeight/2 and posY>=bedHeight/-2:
                if self.data.units == "INCHES":
                    posX=round(posX,4)
                    posY=round(posY,4)
                else:
                    # convert to mm
                    posX=round(posX*25.4,4)
                    posY=round(posY*25.4,4)
                self.data.gcode_queue.put(
                    "G90 G00 X"
                    + str(posX)
                    + " Y"
                    + str(posY)
                    + " "
                )
                return True
            return False
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def processGCode(self):
        '''
        Queues the gcode that puts the sled, bit and controller where the file expects them at the current gcode
        index, so a run can start there.
        '''
        self.queueRestartSequence(self.gcodeStateAt(self.data.gcodeIndex))

    def gcodeStateAt(self, index):
        '''
        Parses the file up to index for the units, positioning, XYZ target (XY include home), modal feed, plunge
        feed, tool, spindle, laser and dwell in effect there, all in the units in effect there.
        '''
        positioning = "G90 "
        homeX = float(self.data.config.getValue("Advanced Settings", "homeX"))
        homeY = float(self.data.config.getValue("Advanced Settings", "homeY"))
        # homeX/homeY are stored in machine units, so a file without G20/G21 runs in them too.
        units = "G20 " if self.data.units == "INCHES" else "G21 "
        defaultFeedScale = 1 / 25.4 if units == "G20 " else 1
        zAxisFeedRate = DEFAULT_PLUNGE_FEED_MM_PER_MIN * defaultFeedScale
        feedRate = float(self.data.config.getValue("Advanced Settings", "maxFeedrate")) * defaultFeedScale
        previousUnits = units
        xpos = homeX
        ypos = homeY
        # None until the file sets Z for the current tool; a restart must not lower the bit to a guess.
        zpos = None
        tool = None
        nextTool = None
        spindle = None
        laser = None
        dwell = None
        for x in range(index):
            filtersparsed = re.sub(r'\(([^)]*)\)', '', self.data.gcode[x])  # replace mach3 style gcode comments with newline
            filtersparsed = re.sub(r';([^\n]*)?', '\n', filtersparsed)  # replace standard ; initiated gcode comments with nothing
            if not filtersparsed.isspace():  # if all spaces, don't send.  likely a comment. #self.data.gcode[x][0] != "(":
                #lines = self.data.gcode[x].split(" ")
                namedTool = toolNamedBy(filtersparsed)
                nextTool = nextTool if namedTool is None else namedTool
                tool = str(nextTool) if changesTool(filtersparsed) and nextTool is not None else tool
                lines = re.sub(r"^\s*N[0-9]+", "", filtersparsed).split(" ")
                if lines:
                    finalLines = []
                    # I think this splits everything up and reassembles them so that each line starts with a M, G or T
                    # I must of had a reason to do so
                    for line in lines:
                        if len(line) > 0:
                            if line[0] == "M" or line[0] == "G" or line[0] == "T" or len(finalLines) == 0:
                                finalLines.append(line)
                            else:
                                finalLines[-1] = finalLines[-1] + " " + line
                    # start processing the lines, keeping track of the state variables.
                    for line in finalLines:
                        # A line of bare axis words (X10 Y5) continues the modal move.
                        if line[0] in "GXYZF":
                            gCodes = gcodeNumbers("G", line)
                            if 90 in gCodes:
                                positioning = "G90 "
                            if 91 in gCodes:
                                positioning = "G91 "
                            if 20 in gCodes:
                                units = "G20 "
                            if 21 in gCodes:
                                units = "G21 "
                            if units != previousUnits:
                                scale = 1 / 25.4 if units == "G20 " else 25.4
                                homeX, homeY, xpos, ypos, zpos, feedRate, zAxisFeedRate = [
                                    None if value is None else value * scale
                                    for value in (homeX, homeY, xpos, ypos, zpos, feedRate, zAxisFeedRate)
                                ]
                                previousUnits = units
                            if line.find("X") != -1:
                                _xpos = re.search("X(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                                if positioning == "G91 ":
                                    xpos = xpos+float(_xpos.groups()[0])
                                else:
                                    xpos = float(_xpos.groups()[0])+homeX
                            if line.find("Y") != -1:
                                _ypos = re.search("Y(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                                if positioning == "G91 ":
                                    ypos = ypos+float(_ypos.groups()[0])
                                else:
                                    ypos = float(_ypos.groups()[0])+homeY
                            if line.find("Z") != -1:
                                _zpos = re.search("Z(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                                if positioning == "G91 ":
                                    zpos = None if zpos is None else zpos + float(_zpos.groups()[0])
                                else:
                                    zpos = float(_zpos.groups()[0])
                            if 4 in gCodes:
                                dwell = re.sub(r"^G0*4", "", line).strip()
                            if line.find("F") != -1:
                                _feedRate = re.search("F(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                                feedRate = float(_feedRate.groups()[0])
                                # A rapid's F is not a plunge feed, though the firmware keeps it as the modal F.
                                if line.find("Z") != -1 and 0 not in gCodes:
                                    zAxisFeedRate = feedRate
                        if line[0] == 'M':
                            mCodes = gcodeNumbers("M", line)
                            spindle = next((f"M{code} " for code in (3, 4, 5) if code in mCodes), spindle)
                            laser = "M106 " if 106 in mCodes else laser
                            laser = "M107 " if 107 in mCodes or 16 in mCodes else laser
                            zpos = None if 6 in mCodes else zpos
        return {
            "units": units, "positioning": positioning, "x": xpos, "y": ypos, "z": zpos,
            "feedRate": feedRate, "zFeedRate": zAxisFeedRate,
            "tool": tool, "spindle": spindle, "laser": laser, "dwell": dwell,
        }

    def queueRestartSequence(self, state, zTarget=None, changeTool=True):
        '''
        Raises the bit to safe height, moves to the state's XY, restores tool, spindle, laser and dwell, lowers the bit
        to zTarget (the state's Z by default) at the plunge feed, then restores the file's modal feed and positioning.
        '''
        zAxisSafeHeight = float(self.data.config.getValue("Maslow Settings", "zAxisSafeHeight"))
        if state["units"] == "G20 ":
            zAxisSafeHeight = zAxisSafeHeight / 25.4
        self.data.gcode_queue.put(state["units"])
        self.data.gcode_queue.put("G90 ")
        # G0 runs at the firmware's maximum and ignores F, but would still set the modal F.
        self.data.gcode_queue.put("G0 Z" + str(round(zAxisSafeHeight, 4)) + " ")
        self.data.gcode_queue.put("G0 X" + str(round(state["x"], 4)) + " Y" + str(round(state["y"], 4)) + " ")
        if changeTool and state["tool"] is not None:
            self.data.gcode_queue.put("T" + state["tool"] + " M6 ")
        if state["spindle"] is not None:
            self.data.gcode_queue.put(state["spindle"])
        if state["laser"] is not None:
            self.data.gcode_queue.put(state["laser"])
        if state["dwell"] is not None:
            self.data.gcode_queue.put("G4 " + state["dwell"])
        zpos = state["z"] if zTarget is None else zTarget
        if zpos is not None:
            self.data.gcode_queue.put("G1 Z" + str(round(zpos, 4)) + " F" + str(round(state["zFeedRate"], 4)) + " ")
        self.data.gcode_queue.put("F" + str(round(state["feedRate"], 4)) + " ")
        self.data.gcode_queue.put(state["positioning"])

    def findPositionAt(self, index):
        '''
        Find the x,y,z gcode position at a given index when simple calcs don't work.  Parse through the file.
        :param index:
        :return:
        '''
        xpos = 0
        ypos = 0
        zpos = 0
        positioning = "G90 "
        for x in range(index):
            filtersparsed = re.sub(r'\(([^)]*)\)', '', self.data.gcode[x])  # replace mach3 style gcode comments with newline
            filtersparsed = re.sub(r';([^\n]*)?', '\n', filtersparsed)  # replace standard ; initiated gcode comments with ""
            if not filtersparsed.isspace():  # if all spaces, don't send.  likely a comment.  #self.data.gcode[x][0] != "(":
                listOfLines = filter(None, re.split("(G)", filtersparsed)) # self.data.gcode[x]))  # self.data.gcode[x].split("G")
                # it is necessary to split the lines along G commands so that commands concatenated on one line
                # are processed correctly
                for line in listOfLines:
                    line = 'G'+line
                    if line[0] == 'G':
                        if line.find("G90") != -1:
                            positioning = "G90 "
                        if line.find("G91") != -1:
                            positioning = "G91 "
                        if line.find("X") != -1:
                            _xpos = re.search("X(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                            if positioning == "G91 ":
                                xpos = xpos + float(_xpos.groups()[0])
                            else:
                                xpos = float(_xpos.groups()[0])
                        if line.find("Y") != -1:
                            _ypos = re.search("Y(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                            if positioning == "G91 ":
                                ypos = ypos + float(_ypos.groups()[0])
                            else:
                                ypos = float(_ypos.groups()[0])
                        if line.find("Z") != -1:
                            _zpos = re.search("Z(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                            if positioning == "G91 ":
                                zpos = zpos + float(_zpos.groups()[0])
                            else:
                                zpos = float(_zpos.groups()[0])
                        if line.find("F") != -1:
                            _fval = re.search("F(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
                            self.data.feedRate=float(_fval.groups()[0])
        #print("xpos="+str(xpos)+", ypos="+str(ypos)+", zpoz="+str(zpos)+" for index="+str(index))
        return xpos, ypos, zpos

    def adjustChain(self, chain):
        '''
        Called during the chain extend routine
        :param chain:
        :return:
        '''
        try:
            for x in range(6):
                self.data.ui_queue1.put("Action", "updateTimer", chain+":"+str(5-x))
                self.data.console_queue.put("Action:updateTimer_" + chain + ":" + str(5 - x))
                time.sleep(1)
            if chain == "left":
                self.data.gcode_queue.put("B02 L1 R0 ")
                self.data.measureRequest = self.issueStopCommand
                self.data.gcode_queue.put("B10 L")
            if chain == "right":
                self.data.gcode_queue.put("B02 L0 R1 ")
                self.data.measureRequest = self.issueStopCommand
                self.data.gcode_queue.put("B10 R")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def issueStopCommand(self, distance):
        try:
            self.data.quick_queue.put("!")
            with self.data.gcode_queue.mutex:
                self.data.gcode_queue.queue.clear()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def toggleCamera(self):
        '''
        Turns camera on or off.  If its suspended, it's read (though I can't explain why at this moment).
        :return:
        '''
        try:
            status = self.data.camera.status()
            if status == "stopped":
                self.data.camera.start()
            if status == "suspended":
                self.data.camera.read()
            if status == "running":
                self.data.camera.stop()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def cameraStatus(self):
        '''
        Sends the status of the camera to the UI client.  Not sure why its not called requestCameraStatus
        Todo: update name to request cameraStatus
        :return:
        '''
        try:
            status = self.data.camera.status()
            if status == "stopped":
                self.data.ui_queue1.put("Action", "updateCamera", "off")
            if status == "suspended":
                self.data.ui_queue1.put("Action", "updateCamera", "off")
            if status == "running":
                self.data.ui_queue1.put("Action", "updateCamera", "on")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def queryCamera(self):
        '''
        Query the camera's settings.  Probably could be called directly by processAction
        Todo: move to processAction
        :return:
        '''
        try:
            self.data.camera.getSettings()
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def velocityPIDTest(self, parameters):
        '''
        Send commands to start the veloctiy pid test
        Todo: further explain this
        :param parameters:
        :return:
        '''
        try:
            print(parameters)
            print(parameters["KpV"])
            self.data.config.setValue("Advanced Settings", "KpV", parameters["KpV"])
            self.data.config.setValue("Advanced Settings", "KiV", parameters["KiV"])
            self.data.config.setValue("Advanced Settings", "KdV", parameters["KdV"])
            gcodeString = "B13 "+parameters["vMotor"]+"1 S"+parameters["vStart"]+" F"+parameters["vStop"]+" I"+parameters["vSteps"]+" V"+parameters["vVersion"]
            print(gcodeString)
            self.data.PIDVelocityTestVersion = parameters["vVersion"]
            self.data.gcode_queue.put(gcodeString)
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def positionPIDTest(self, parameters):
        '''
        Send commands to start the position pid test
        Todo: further explain this
        :param parameters:
        :return:
        '''

        try:
            print(parameters)
            print(parameters["KpP"])
            self.data.config.setValue("Advanced Settings", "KpPos", parameters["KpP"])
            self.data.config.setValue("Advanced Settings", "KiPos", parameters["KiP"])
            self.data.config.setValue("Advanced Settings", "KdPos", parameters["KdP"])

            gcodeString = "B14 "+parameters["pMotor"]+"1 S"+parameters["pStart"]+" F"+parameters["pStop"]+" I"+parameters["pSteps"]+" T"+parameters["pTime"]+" V"+parameters["pVersion"]
            print(gcodeString)
            self.data.PIDPositionTestVersion = parameters["pVersion"]
            self.data.gcode_queue.put(gcodeString)
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def velocityPIDTestRun(self, command, msg):
        '''

        :param command:
        :param msg:
        :return:
        '''
        try:
            if command == 'stop':
                self.data.inPIDVelocityTest = False
                print("PID velocity test stopped")
                print(self.data.PIDVelocityTestData)
                data = json.dumps({"result": "velocity", "version": self.data.PIDVelocityTestVersion, "data": self.data.PIDVelocityTestData})
                self.data.ui_queue1.put("Action", "updatePIDData", data)
                self.stopRun()
            if command == 'running':
                if msg.find("Kp=") == -1:
                    if self.data.PIDVelocityTestVersion == "2":
                        if msg.find("setpoint") == -1:
                            self.data.PIDVelocityTestData.append(msg)
                    else:
                        self.data.PIDVelocityTestData.append(float(msg))
            if command == 'start':
                self.data.inPIDVelocityTest = True
                self.data.PIDVelocityTestData = []
                print("PID velocity test started")
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def positionPIDTestRun(self, command, msg):
        try:
            if command == 'stop':
                self.data.inPIDPositionTest = False
                print("PID position test stopped")
                print(self.data.PIDPositionTestData)
                data = json.dumps({"result": "position", "version": self.data.PIDPositionTestVersion, "data": self.data.PIDPositionTestData})
                self.data.ui_queue1.put("Action", "updatePIDData", data)
                self.stopRun()
            if command == 'running':
                if msg.find("Kp=") == -1:
                    if self.data.PIDPositionTestVersion == "2":
                        if msg.find("setpoint") == -1:
                            self.data.PIDPositionTestData.append(msg)
                    else:
                        self.data.PIDPositionTestData.append(float(msg))
            if command == 'start':
                self.data.inPIDPositionTest = True
                self.data.PIDPositionTestData = []
                print("PID position test started")
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def updateGCode(self, gcode):
        try:
            #print(gcode)
            homeX = float(self.data.config.getValue("Advanced Settings", "homeX"))
            homeY = float(self.data.config.getValue("Advanced Settings", "homeY"))

            if self.data.units == "MM":
                scaleFactor = 25.4
            else:
                scaleFactor = 1.0
            self.data.gcodeShift = [
                homeX,
                homeY
            ]

            self.data.gcodeFile.loadUpdateFile(gcode)
            self.data.ui_queue1.put("Action", "gcodeUpdate", "")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def downloadDiagnostics(self):
        try:
            timestr = time.strftime("%Y%m%d-%H%M%S")
            filename = self.data.config.home+"/.WebControl/"+"wc_diagnostics_"+timestr+".zip"
            zipObj = zipfile.ZipFile(filename, 'w')
            path1 = self.data.config.home+"/.WebControl/webcontrol.json"
            zipObj.write(path1, os.path.basename(path1))
            path1 = self.data.config.home + "/.WebControl/alog.txt"
            zipObj.write(path1, os.path.basename(path1))
            path1 = self.data.config.home + "/.WebControl/log.txt"
            zipObj.write(path1, os.path.basename(path1))
            for previous in ("log.txt.1", "alog.txt.1"):
                path1 = self.data.config.home + "/.WebControl/" + previous
                if os.path.exists(path1):
                    zipObj.write(path1, previous)
            zipObj.close()
            keepNewest(self.data.config.home + "/.WebControl/wc_diagnostics_*.zip", KEEP_ARCHIVES)
            return filename
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def clearLogs(self):
        try:
            retval = self.data.logger.deleteLogFiles()
            return retval
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
    '''
    def checkForLatestPyRelease(self):
        if True: #self.data.platform=="PYINSTALLER":
            print("check for pyrelease")
            g = Github()
            repo = g.get_repo("madgrizzle/WebControl")
            releases = repo.get_releases()
            latest = 0
            latestRelease = None
            type = self.data.pyInstallType
            platform = self.data.pyInstallPlatform
            for release in releases:
                try:
                    tag_name = re.sub(r'[v]',r'',release.tag_name)
                    print(release.body)
                    #print(tag_name)
                    if float(tag_name) > latest:
                        latest = float(tag_name)
                        latestRelease = release

                except:
                    print("error parsing tagname")
            print(latest)
            if latest>self.data.pyInstallCurrentVersion:
                if latestRelease is not None:
                    print(latestRelease.tag_name)
                    assets = latestRelease.get_assets()
                    for asset in assets:
                        if asset.name.find(type) != -1 and asset.name.find(platform) != -1:
                            print(asset.name)
                            print(asset.url)
                            self.data.ui_queue1.put("Action", "pyinstallUpdate", "on")
                            self.data.pyInstallUpdateAvailable = True
                            self.data.pyInstallUpdateBrowserUrl = asset.browser_download_url
                            self.data.pyInstallUpdateVersion = latest

    def processAbsolutePath(self, path):
        index = path.find("main.py")
        self.data.pyInstallInstalledPath = path[0:index-1]
        print(self.data.pyInstallInstalledPath)
    
    def updatePyInstaller(self):
        home = self.data.config.getHome()
        if self.data.pyInstallUpdateAvailable == True:
            if not os.path.exists(home+"/.WebControl/downloads"):
                print("creating downloads directory")
                os.mkdir(home+"/.WebControl/downloads")
            fileList=glob.glob(home+"/.WebControl/downloads/*.gz")
            for filePath in fileList:
                try:
                    os.remove(filePath)
                except:
                    print("error cleaning download directory: ",filePath)
                    print("---")
            if self.data.pyInstallPlatform == "win32" or self.data.pyInstallPlatform == "win64":
                filename = wget.download(self.data.pyInstallUpdateBrowserUrl, out=home+"\\.WebControl\\downloads")
            else:
                filename = wget.download(self.data.pyInstallUpdateBrowserUrl, out=home+"/.WebControl/downloads")
            print(filename)
            
            if self.data.platform == "PYINSTALLER":
                lhome = os.path.join(self.data.platformHome)
            else:
                lhome = "."
            if self.data.pyInstallPlatform == "win32" or self.data.pyInstallPlatform == "win64":
                path = lhome+"/tools/upgrade_webcontrol_win.bat"
                copyfile(path, home+"/.WebControl/downloads/upgrade_webcontrol_win.bat")
                path = lhome+"/tools/7za.exe"
                copyfile(path, home+"/.WebControl/downloads/7za.exe")
                self.data.pyInstallInstalledPath = self.data.pyInstallInstalledPath.replace('/','\\')
                program_name = home+"\\.WebControl\\downloads\\upgrade_webcontrol_win.bat"
                
            else:
                path = lhome+"/tools/upgrade_webcontrol.sh"
                copyfile(path, home+"/.WebControl/downloads/upgrade_webcontrol.sh")
                program_name = home+"/.WebControl/downloads/upgrade_webcontrol.sh"
                self.make_executable(home+"/.WebControl/downloads/upgrade_webcontrol.sh")
            tool_path = home+"\\.WebControl\\downloads\\7za.exe"
            arguments = [filename, self.data.pyInstallInstalledPath, tool_path]
            command = [program_name]
            command.extend(arguments)
            print("popening")
            print(command)
            subprocess.Popen(command)
            return True
        return False

    def make_executable(self, path):
        print("1")
        mode = os.stat(path).st_mode
        print("2")
        mode |= (mode & 0o444) >> 2    # copy R bits to X
        print("3")
        os.chmod(path, mode)
        print("4")
    '''

    def addDirToZip(self, zipHandle, path, basePath=""):
        basePath = basePath.rstrip("\\/") + ""
        basePath = basePath.rstrip("\\/")
        for root, dirs, files in os.walk(path):
            # add dir itself (needed for empty dirs
            zipHandle.write(os.path.join(root, "."))
            # add files
            for file in files:
                filePath = os.path.join(root, file)
                inZipPath = filePath.replace(basePath, "", 1).lstrip("\\/")
                # print filePath + " , " + inZipPath
                zipHandle.write(filePath, inZipPath)

    def zipfolder(self, filename, target_dir):
        zipobj = zipfile.ZipFile(filename, 'w', zipfile.ZIP_DEFLATED)
        rootlen = len(target_dir) + 1
        for base, dirs, files in os.walk(target_dir):
            for file in files:
                fn = os.path.join(base, file)
                zipobj.write(fn, fn[rootlen:])
        zipobj.close()


    def backupWebControl(self):
        try:
            timestr = time.strftime("%Y%m%d-%H%M%S")
            home = self.data.config.getHome()
            filename = home+"/wc_backup_"+timestr+".zip"
            print(filename)
            folder = self.data.config.home+'/.WebControl'
            print(folder)
            self.zipfolder(filename, folder)
            keepNewest(home + "/wc_backup_*.zip", KEEP_ARCHIVES)
            return filename
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False


    def restoreWebControl(self, fileName):
        try:
            with zipfile.ZipFile(fileName, 'r') as zipObj:
                if "webcontrol.json" not in zipObj.namelist():
                    self.data.console_queue.put("Restore file has no webcontrol.json")
                    return False
                if not isinstance(json.loads(zipObj.read("webcontrol.json")), dict):
                    self.data.console_queue.put("Restore file's webcontrol.json holds no settings")
                    return False
                zipObj.extractall(os.path.join(self.data.config.home, ".WebControl"))
            retval = self.data.config.reloadWebControlJSON()
            if retval is True:
                self.data.gcode_queue.put("$$")
            return retval
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False

    def setFakeServo(self, value):
        try:
            if value:
                self.data.gcode_queue.put("B99 ON")
            else:
                self.data.gcode_queue.put("B99 OFF")
            return True
        except Exception as e:
            self.data.console_queue.put(str(e))
            return False
