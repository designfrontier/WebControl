from collections import deque
import time
import re

import serial

from Connection.firmwareGcode import pausesFirmware, toolChangeLine
from DataStructures.makesmithInitFuncs import MakesmithInitFuncs


class SerialPortThread(MakesmithInitFuncs):
    """

    SerialPort is the thread which handles direct communication with the CNC machine.
    SerialPort initializes the connection and then receives
    and parses messages. These messages are then passed to the main thread via the message_queue
    queue where they are added to the GUI

    """

    bufferSize = 126  # The total size of the arduino buffer
    weAreBufferingLines = 0
    # Minimum time between lines sent to allow Arduino to cope
    # could be smaller (0.02) however larger number doesn't seem to impact performance
    MINTimePerLine = 0.05
    RESYNC_DELAY = 0.25

    serialInstance = None

    def __init__(self):
        self.machineIsReadyForData = False  # Tracks whether last command was acked
        self.lastWriteTime = time.time()
        self.bufferSpace = self.bufferSize  # The amount of space currently available in the buffer
        self.unackedLines = deque()  # (length, gcode index or None) per line sent, newest on the left
        self.resyncAfter = None
        self.caughtUpSinceHalt = False

    def _write(self, message, isQuickCommand=False, gcodeIndex=None):
        '''
        Sends messages to controller.  No change from what I can tell from ground control
        :param message:
        :param isQuickCommand:
        :param gcodeIndex: the file line being sent, if any, so a halt can rewind to it.
        :return:
        '''
        taken = time.time() - self.lastWriteTime
        if taken < self.MINTimePerLine:  # wait between sends
            time.sleep(self.MINTimePerLine)  # could use (taken - MINTimePerLine)

        message = message + "\n"

        if len(message) > 1:
            self.data.console_queue.put("Sending: " + str(message).rstrip('\n'))
        
            self.bufferSpace = self.bufferSpace - len(
                message
            )  # shrink the available buffer space by the length of the line

            self.machineIsReadyForData = False

            # if this is a quick message sent as soon as the button is pressed (like stop) then put it on the right side of the queue
            # because it is the first message sent, otherwise put it at the end (left) because it is the last message sent
            if isQuickCommand:
                if message[0] == "!":
                    self._awaitResync()
                else:
                    self.unackedLines.append((len(message), None))
            else:
                self.unackedLines.appendleft((len(message), gcodeIndex))

            '''
            Monitor gcode for positioning mode change requests
            '''
            positioningMode = self._positioningModeSetBy(message)

            message = message.encode()

            '''
            try sending message
            '''
            try:
                self.serialInstance.write(message)
                self.data.serialRecorder.tx(message.decode())
                # Update positioning mode after message has been sent.
                # In 'try' block to maintain state integrity if message send fails.
                if (positioningMode is not None):
                    self.data.positioningMode = positioningMode
                    #print("Set positioning mode: " + str(positioningMode))
                self.data.logger.writeToLog("Sent: " + str(message.decode()))
            except:
                self.data.console_queue.put("write issue")
                self.data.logger.writeToLog("Send FAILED: " + str(message.decode()))

            self.lastWriteTime = time.time()
        else:
            self.data.console_queue.put("Skipping: " + str(message).rstrip('\n'))

    def _awaitResync(self):
        '''
        The controller has flushed its buffer, but acks sent before it did may still be in flight or unread. Ignore
        acks and hold regular lines until a status report read after the input ran dry past the resync delay.
        '''
        self._emptyAccounting()
        self.machineIsReadyForData = False
        self.resyncAfter = time.time() + self.RESYNC_DELAY
        self.caughtUpSinceHalt = False

    def _rewindToDroppedLines(self):
        '''
        The controller drops every line it hadn't finished, so a resume must start from the oldest of them.
        '''
        dropped = [index for _length, index in self.unackedLines if index is not None]
        if dropped and self.data.jobInProgress:
            self.data.gcodeIndex = min(dropped)
            self.data.actions.sendGCodePositionUpdate(self.data.gcodeIndex, recalculate=True)

    def _emptyAccounting(self):
        self.unackedLines.clear()
        self.bufferSpace = self.bufferSize

    def _startRecording(self):
        try:
            config = self.data.config
            settings = {section: {option["key"]: option["value"] for option in config.settings.get(section, [])}
                        for section in ("Maslow Settings", "Advanced Settings")}
            home = {"x": float(config.getValue("Advanced Settings", "homeX")),
                    "y": float(config.getValue("Advanced Settings", "homeY"))}
            self.data.serialRecorder.startSession(self.data.comport, self.data.units, home, settings, self.data.version)
        except Exception as e:
            self.data.console_queue.put("Could not start the serial recording: " + str(e))

    def _setupMachineUnits(self):
        '''
        Send command to put controller in correct units state.
        :return:
        '''
        if self.data.units == "INCHES":
            self.data.gcode_queue.put("G20 ")
        else:
            self.data.gcode_queue.put("G21 ")

    def _requestSettingsUpdate(self):
        '''
        Send command to have controller report settings
        :return:
        '''
        self.data.gcode_queue.put("$$")

    def _stripComments(self, line):
        filtersparsed = re.sub(r'\(([^)]*)\)', '', line)  # mach3 style ( ) comments
        return re.sub(r';([^\n]*)?', '', filtersparsed)  # standard ; comments

    def _sanitise(self, line):
        '''
        Strips comments, and the characters the firmware acts on as realtime commands anywhere in a line.
        '''
        return re.sub(r'[!~`?\x00-\x1f\x7f]', '', self._stripComments(line))

    def _positioningModeSetBy(self, line):
        '''
        :return: 0 if the line switches to absolute (G90), 1 if to relative (G91), None if it does neither.
        '''
        findG90 = line.rfind("G90")
        findG91 = line.rfind("G91")
        if findG90 == findG91:
            return None
        return 0 if findG90 > findG91 else 1

    def _positioningModeOf(self, line):
        mode = self._positioningModeSetBy(line)
        return self.data.positioningMode if mode is None else mode

    def _shiftToHome(self, line):
        '''
        Applies the gcode home shift, which only makes sense for absolute (G90) moves.
        '''
        return self.data.gcodeFile.moveLine(line) if self._positioningModeOf(line) == 0 else line

    def _problemWith(self, line):
        '''
        :param line: the line exactly as it will be written, without its newline.
        :return: (title, reason, advice) if the controller can't safely take the line, else None.
        '''
        if "(" in line:
            # 51.27 loops forever on a comment.
            return ("Unclosed Comment", "has an unclosed comment, which hangs the controller.",
                    "Resume stops here again: stop the job, fix the file and load it again.")
        written = len(line) + 1
        if written > self.bufferSize:
            # The firmware's ring overflows and it then runs the line's unread tail as a command.
            return ("Too Long", "is " + str(written) + " characters; the controller accepts at most "
                    + str(self.bufferSize) + ".",
                    "Resume stops here again: stop the job, fix the file (e.g. enable Truncate Floating Point "
                    "Numbers) and load it again.")
        return None

    def _streamProblem(self, line):
        return None if line.isspace() else self._problemWith(self._shiftToHome(line))

    def _refuseStreamLine(self, problem):
        title, reason, advice = problem
        self.data.ui_queue1.put("Alert", "Line " + title,
                                "Line " + str(self.data.gcodeIndex + 1) + " " + reason + " " + advice)
        self.data.actions.pauseRun()

    def sendNextLine(self):
        """
            Sends the next line of gcode to the machine
        """
        if self.data.gcodeIndex < len(self.data.gcode):
            runGeneration = self.data.runGeneration
            line = self._sanitise(self.data.gcode[self.data.gcodeIndex])
            problem = self._streamProblem(toolChangeLine(line, self.data.nextTool, self.data.currentTool)[0])
            if problem is not None:
                return self._refuseStreamLine(problem)
            # check if command is going to be issued that pauses the controller.
            self.managePause(line)
            line = self.manageToolChange(line)
            if not line.isspace(): # if all spaces, don't send.  likely a comment.
                positioningMode = self._positioningModeOf(line)
                line = self._shiftToHome(line)
                self._write(line, gcodeIndex=self.data.gcodeIndex)
                try:
                    self._afterLineSent(line, positioningMode)
                except Exception as e:
                    # The line is already written, so the index must still advance or the line goes out again.
                    self.data.console_queue.put("Error after sending gcode line: " + str(e))

            if self.data.runGeneration != runGeneration:
                return
            # increment gcode index
            if self.data.gcodeIndex + 1 < len(self.data.gcode):
                self.data.gcodeIndex = self.data.gcodeIndex + 1
            else:
                self.data.uploadFlag = 0
                self.data.jobInProgress = False
                self.data.gcodeIndex = 0
                self.data.console_queue.put("Gcode Ended")

    def _afterLineSent(self, line, positioningMode):
        # track current target Z-Axis position
        z = re.search("Z(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)", line)
        if z:
            zWord = float(z.groups()[0])
            self.data.currentZTarget = zWord if positioningMode == 0 else self.data.currentZTarget + zWord
        # if there is a units change, then update the UI client so position messages are processed correctly.
        if line.find("G20") != -1:
            if self.data.units != "INCHES":
                self.data.actions.updateSetting("toInches", 0, True)  # value = doesn't matter
        if line.find("G21") != -1:
            if self.data.units != "MM":
                self.data.actions.updateSetting("toMM", 0, True)  # value = doesn't matter
        # send a gcode update to UI client.
        self.data.actions.sendGCodePositionUpdate(self.data.gcodeIndex)

    def managePause(self, line):
        '''
        Stops streaming after an M0/M1 line; the firmware then waits for the "~" that Resume sends.
        '''
        if not pausesFirmware(line):
            return
        if self.data.uploadFlag == 1:
            self.data.uploadFlag = -1
        self.data.pausedUnits = self.data.units
        self.data.ui_queue1.put("Action", "setAsResume", "")

    def manageToolChange(self, line):
        '''
        Tracks the tool the file selects and stops streaming after an M6 that changes it, which pauses the firmware.
        :return: the line to send, led by the T of the tool an M6 line changes to.
        '''
        line, self.data.nextTool, pauses = toolChangeLine(line, self.data.nextTool, self.data.currentTool)
        if pauses:
            self.data.toolBeforeChange = (
                self.data.currentTool if self.data.toolBeforeChange is None else self.data.toolBeforeChange
            )
            self.data.currentTool = self.data.nextTool
            self.data.pausedUnits = self.data.units
            if self.data.uploadFlag == 1:
                self.data.uploadFlag = -1
        return line

    def closeConnection(self):
        '''
        Closes the serial connection and updates the status.  Needed for firmware upgrades.
        :return:
        '''
        if self.serialInstance is not None:
            self.serialInstance.close()
            self.data.serialPort.serialPortRequest = "Closed"
            print("connection closed at serialPortThread")
        else:
            print("serial Instance is none??")
        return


    def getmessage(self):
        '''
        Opens a serial connection called self.serialInstance
        Processes all the messages from controller.
        :return:
        '''

        # check for serial version being > 3
        if float(serial.VERSION[0]) < 3:
            self.data.ui_queue1.put("Alert", "Incompability Detected",
                "Pyserial version 3.x is needed, but version "
                + serial.VERSION
                + " is installed"
            )

        self.weAreBufferingLines = bool(int(self.data.config.getValue("Maslow Settings", "bufferOn")) )

        # Opening the port resets the controller, which would break a firmware upgrade in progress.
        if self.data.serialPort.serialPortRequest == "requestToClose":
            return

        try:
            self.data.comport = self.data.config.getValue("Maslow Settings", "COMport")
            connectMessage = "Trying to connect to controller on " +self.data.comport
            self.data.console_queue.put(connectMessage)
            self.serialInstance = serial.Serial(
                self.data.comport, 57600, timeout=0.25
            )  # self.data.comport is the com port which is opened
        except:
            #self.data.console_queue.put(self.data.comport + " is unavailable or in use")
            pass
        else:
            self.data.console_queue.put("\r\nConnected on port " + self.data.comport + "\r\n")
            self.data.ui_queue1.put("Action", "connectionStatus", {'status': 'connected', 'port': self.data.comport, 'fakeServoStatus': self.data.fakeServoStatus})
            self.data.ui_queue1.put("TextMessage", "", "Connected on port " + self.data.comport)

            self.serialInstance.parity = (
                serial.PARITY_ODD
            )  # This is something you have to do to get the connection to open properly. I have no idea why.
            self.serialInstance.close()
            self.serialInstance.open()
            self.serialInstance.close()
            self.serialInstance.parity = serial.PARITY_NONE
            self.serialInstance.open()

            self.lastMessageTime = time.time()
            self.data.connectionStatus = 1
            self._startRecording()
            # The port open resets the board, and its boot banner queues B05 and the write-only settings.
            self._setupMachineUnits()
            self._requestSettingsUpdate()
            # The host may still hold G91 from before a lost connection; resuming relies on the two agreeing.
            self.data.gcode_queue.put("G90 ")
            self.data.currentTool = 0  # This is necessary since the controller will have reset tool to zero.
            self.data.nextTool = 0
            self.data.toolBeforeChange = None
            # Opening the port resets the controller, which ends any pause.
            self.data.firmwarePaused = False
            self.data.firmwareHoldsJobLines = False

            handedToFirmwareUpgrade = False
            try:
                handedToFirmwareUpgrade = self._serviceConnection()
            finally:
                if not handedToFirmwareUpgrade:
                    self.data.connectionStatus = 0

    def _serviceConnection(self):
        '''
        Runs the read/write loop until the connection is lost or closed.
        :return: True if the port was handed to a firmware upgrade, which then owns connectionStatus.
        '''
        while True:
            # check to see if the serial port needs to be closed (for firmware upgrade)
            if self.data.serialPort.serialPortRequest == "requestToClose":
                print("processing request to close")
                self.closeConnection()
                return True

            try:
                self._exchangeLines()
            except Exception as e:
                self.data.console_queue.put("Serial loop error: " + str(e))

            # Check for serial connection loss
            # -------------------------------------------------------------------------------------
            if time.time() - self.lastMessageTime > 5:
                self.data.console_queue.put("Connection Timed Out")
                if self.data.uploadFlag > 0:
                    self.data.ui_queue1.put("Alert", "Connection Lost",
                                            "Message: USB connection lost. This has likely caused the machine to loose it's calibration, which can cause erratic behavior. It is recommended to stop the program, remove the sled, and perform the chain calibration process. Press Continue to override and proceed with the cut.")
                    # Lines in the controller's buffer were lost, so don't stream again until the user resumes.
                    self.data.actions.pauseRun()
                else:
                    self.data.ui_queue1.put("SendAlarm", "Alarm: Connection Failed or Invalid Firmware", "")
                self.data.connectionStatus = 0
                self.serialInstance.close()
                return False

            if self.data.requestSerialClose:
                # close down the serial port.
                self.data.requestSerialClose = False
                self.data.actions.pauseRun()
                self.data.connectionStatus = 0
                self.serialInstance.close()
                return False

            time.sleep(0.01)

    def _exchangeLines(self):
        '''
        Reads one line from the controller if available, then writes whatever the controller has room for.
        '''
        lineFromMachine = ""

        try:
            if self.serialInstance.in_waiting > 0:
                # the need for the .decode() at the end MIGHT be a python3 thing.  ground control doesn't have
                # it.
                lineFromMachine = self.serialInstance.readline().decode()
                self.data.serialRecorder.rx(lineFromMachine)
                self.lastMessageTime = time.time()
                self.data.message_queue.put(lineFromMachine)
            elif self.resyncAfter is not None and time.time() >= self.resyncAfter:
                self.caughtUpSinceHalt = True
        except:
            pass

        if self.resyncAfter is not None:
            if lineFromMachine.startswith("<") and self.caughtUpSinceHalt:
                self.resyncAfter = None
                self._emptyAccounting()
                self.machineIsReadyForData = True
        # Check if a line has been completed
        elif lineFromMachine == "ok\r\n" or (len(lineFromMachine) >= 6 and lineFromMachine[0:6] == "error:"):
            self.machineIsReadyForData = True
            if bool(self.unackedLines) is True:  # if we've sent lines to the machine
                # free up that space in the buffer
                self.bufferSpace = (self.bufferSpace + self.unackedLines.pop()[0])
        # The firmware stops itself on these and never acks the lines it held.
        if lineFromMachine.startswith(("ALARM:", "Buffer overflow!")):
            self._rewindToDroppedLines()
            self._awaitResync()


        # Write to the machine if ready
        # -------------------------------------------------------------------------------------

        # send any emergency instructions to the machine if there are any
        if self.data.quick_queue.empty() != True:
            command = self.data.quick_queue.get_nowait()
            self._write(command, True)

        # send regular instructions to the machine if there are any
        #print("bSpace="+str(self.bufferSpace)+", bSize="+str(self.bufferSize)+", ready="+str(self.machineIsReadyForData))
        if self.bufferSpace == self.bufferSize:
            if self.data.gcode_queue.empty() != True:
                if self.machineIsReadyForData:
                    self._sendQueuedCommand(self.data.gcode_queue.get_nowait())
        # Send the next line of gcode to the machine if we're running a program and uploadFlag is enabled. Will
        # send lines to buffer if there is space and the feature is turned on
        # Also, don't send if there's still data in gcode_queue.
        if (self.data.uploadFlag == 1 and len(self.data.gcode) > 0 and self.data.gcode_queue.empty()
                and self.resyncAfter is None):
            if self.weAreBufferingLines:
                try:
                    line = self._sanitise(self.data.gcode[self.data.gcodeIndex])
                    toSend = toolChangeLine(line, self.data.nextTool, self.data.currentTool)[0]
                    # sendNextLine refuses a problem line at once, so it needn't wait for room.
                    fits = (line.isspace() or self._streamProblem(toSend) is not None
                            or self.bufferSpace >= len(self._shiftToHome(toSend) + "\n"))
                    if fits:
                        self.sendNextLine()
                except IndexError:
                    self.data.console_queue.put("index error when reading gcode")
                    # we don't want the whole serial thread to close if the gcode can't be sent because of an
                    # index error (file deleted...etc)
            elif self._controllerIsIdle():
                self.sendNextLine()

    def _sendQueuedCommand(self, command):
        command = self._sanitise(command)
        if len(command) == 0:
            return
        command = command + " "
        problem = self._problemWith(toolChangeLine(command, self.data.nextTool, self.data.currentTool)[0])
        if problem is not None:
            title, reason, _advice = problem
            self.data.ui_queue1.put("Alert", "Command " + title, "That command " + reason)
            return
        command = self.manageToolChange(command)
        self._write(command)
        # if units have changed, update settings and notify UI clients to stay in sync
        if command.find("G20") != -1 and self.data.units != "INCHES":
            self.data.actions.updateSetting("toInches", 0, True)  # value = doesn't matter
        if command.find("G21") != -1 and self.data.units != "MM":
            self.data.actions.updateSetting("toMM", 0, True)  # value = doesn't matter
        self.data.actions.sendGCodePositionUpdate(self.data.gcodeIndex, recalculate=True)

    def _controllerIsIdle(self):
        '''
        :return: True when the controller's receive buffer is empty and it has acked the last line.
        '''
        return self.bufferSpace == self.bufferSize and self.machineIsReadyForData

    def extractGcodeValue(self, readString, target, defaultReturn):
        # Reads a string and returns the value of number following the target character.
        # if no number is found, defaultReturn is returned

        begin = readString.find(target)
        end = self.findEndOfNumber(readString, begin + 1)
        numberAsString = readString[begin + 1: end]
        try:
            numberAsFloat = float(numberAsString)
        except:
            return defaultReturn

        if begin == -1:
            return defaultReturn
        else:
            return numberAsFloat

    def findEndOfNumber(self, textString, index):
        # Return the index of the last digit of the number beginning at the index passed in

        i = index
        while i < len(textString):
            if textString[i].isdigit() or textString[i] == '.':
                i = i + 1
            else:
                return i
        return i

