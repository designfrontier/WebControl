from Connection import firmwareFacts
from DataStructures.makesmithInitFuncs import MakesmithInitFuncs

import re
import threading
import time


class MessageProcessor(MakesmithInitFuncs):
    """
    This class processes messages from the controller and sends them to the UI message queue if something needs to get relayed to the user
    """

    def start(self):
        threading.Thread(target=self.checkForUpdatesHourly, daemon=True).start()
        while True:
            # give other threads opportunity to run
            time.sleep(0.001)
            # process messages while queue is not empty.  Everything else is on hold until queue is cleared.
            while (  not self.data.message_queue.empty() ):  # if there is new data to be read
                message = self.data.message_queue.get()
                try:
                    self.processMessage(message)
                except Exception as e:
                    self.data.console_queue.put("Could not process controller message " + repr(message) + ": " + str(e))

    def checkForUpdatesHourly(self):
        while True:
            if time.time()-self.data.lastChecked > 60*60:
                self.data.lastChecked = time.time()
                try:
                    self.data.releaseManager.checkForLatestPyRelease()
                    self.data.helpManager.checkForUpdatedHelp()
                except Exception as e:
                    self.data.console_queue.put("Update check failed: " + str(e))
            time.sleep(60)

    def holdStreaming(self):
        self.data.firmwareHoldsJobLines = self.data.firmwareHoldsJobLines or self.data.uploadFlag > 0
        self.data.uploadFlag = 0

    def watchForBoot(self, message):
        '''
        On each controller boot, asks for the version and rewrites what the firmware only applies on a write.
        '''
        if firmwareFacts.EEPROM_REWRITE_NOTICE in message:
            self.data.firmwareRewriteNoticeSeen = True
        if not firmwareFacts.isBootBanner(message):
            return
        self.data.controllerBoard = firmwareFacts.boardBanner(message) or self.data.controllerBoard
        now = time.time()
        if not self.data.bootInProgress or now - self.data.lastBootBannerTime > firmwareFacts.BOOT_BANNER_WINDOW:
            self.data.firmwareRewriteNoticeSeen = False
            self.data.gcode_queue.put("B05 ")
            [self.data.gcode_queue.put(line) for line in self.data.config.writeOnlyFirmwareSettings()]
        self.data.bootInProgress = not firmwareFacts.endsBoot(message)
        self.data.lastBootBannerTime = now

    def processMessage(self, message):
        self.watchForBoot(message)
        if "EEPROM read fail" in message:
            self.data.config.restoreControllerSettings()
        if message != "":
            if message[0] == "<":
                # message to be processed by UIProcessor
                self.data.ui_controller_queue.put(message)
            elif message[0] == "$":
                # setting received from controller
                self.data.config.receivedSetting(message)
            elif message[0] == "[":
                # positional error or measurement message
                if message[1:4] == "PE:":
                    # send to UIProcessor for processing
                    self.data.ui_controller_queue.put(message)
                elif message[1:8] == "Measure":
                    # get distance and call the callback.
                    measuredDist = float(message[9 : len(message) - 3])
                    try:
                        self.data.measureRequest(measuredDist)
                    except Exception as e:
                        self.data.console_queue.put(str(e))
                        self.data.console_queue.put("No function has requested a measurement")
            elif message[0:14] == "FAKE_SERVO off":
                self.data.fakeServoStatus = False
                self.data.console_queue.put(message)
            elif message[0:13] == "FAKE_SERVO on":
                self.data.fakeServoStatus = True
                self.data.console_queue.put(message)
            elif message[0:13] == "Maslow Paused":
                # The firmware holds every line until "~", which only the user's Resume (or a jog) sends.
                self.holdStreaming()
                self.data.firmwarePaused = True
                self.data.ui_controller_queue.put(message)
            elif message[0:12] == "Tool Change:":
                # This thread owns the tool-change state; the UIProcessor only shows the prompt.
                self.data.manualZAxisAdjust = True
                self.data.toolChangeWaits = True
                self.data.toolBeforeChange = None
                self.holdStreaming()
                self.data.pausedUnits = self.data.units
                self.data.pausedPositioningMode = self.data.positioningMode
                tool = re.search(r"tool\s+([0-9]+)", message)
                if tool:
                    # A Stop can flush an M6 the host already counted, so the firmware's word wins.
                    self.data.currentTool = self.data.nextTool = int(tool.group(1))
                self.data.ui_controller_queue.put(message)
            elif message[0:8] == "Message:":
                # suppress the annoying messages about invalid chain lengths during the calibration process
                if (self.data.calibrationInProcess and message[0:15] == "Message: Unable"):
                    return
                # track the previous uploadFlag and stop uploading.  Likely am going to receive a message
                # that requires a pause.
                self.data.previousUploadStatus = self.data.uploadFlag
                self.holdStreaming()
                if message.find("adjust Z-Axis") != -1:
                    # z-axis must not be enabled.  Send message to UIProcessor for processing.
                    # set manualZAxisAdjust, but not sure its needed if manual adjustment is being used.
                    # Todo: cleanup if needed.
                    self.data.manualZAxisAdjust = True
                    self.data.ui_controller_queue.put(message)
                if message[0:15] == "Message: Unable":
                    # received an error message from forward.kinematics that chain lengths don't resolve.
                    # send to UIProcessor for processing.
                    self.data.ui_controller_queue.put(message)
            elif message[0:6] == "ALARM:" or message[0:16] == "Buffer overflow!":
                # something bad happened, likley sled not keeping up.
                # keep track of upload status, pause run, and send to UIProcessor for processing.
                self.data.previousUploadStatus = self.data.uploadFlag
                self.data.uploadFlag = 0
                self.data.ui_controller_queue.put(message)
            elif message[0:8] == "Firmware":
                '''
                send this alarm clear if Firmware is received.  The thought is that if there is an alarm and
                this message appears, then the connection to the arduino has been reset.
                '''
                self.data.ui_queue1.put("Action", "clearAlarm", "")

                self.data.logger.writeToLog("Ground Control Version " + str(self.data.version) + "\n")
                self.data.console_queue.put("WebControl " + str(self.data.version) + "\r\n" + message + "\r\n")
                self.data.ui_controller_queue.put(message)
                tmpVersion = firmwareFacts.parseFirmwareVersion(message)
                if tmpVersion is None:
                    return
                self.data.controllerFirmwareVersion = tmpVersion
                # <50 implies a stock firmware
                if tmpVersion <50:
                    if self.data.stockFirmwareVersion is not None:
                        if tmpVersion < float(self.data.stockFirmwareVersion):
                            self.data.ui_queue1.put("Alert", "Alert",
                                                    "<p>Warning, you are running a stock firmware that is not up to date.  This version may not work correctly with this version of WebControl.</p>"
                                                    + "</p><p>Please, click Actions->Upgrade Stock Firmware to update the controller to the latest WebControl-compatible code.</p>"
                                                    +"<p>WebControl:"+str(float(self.data.stockFirmwareVersion))+", Controller:"+str(tmpVersion)+"</p>"
                                                    )
                        elif tmpVersion > float(self.data.stockFirmwareVersion):
                            self.data.ui_queue1.put("Alert", "Alert",
                                                    "<p>Warning, you are running a stock firmware that is newer than what is included in WebControl.  This version may not work correctly with this version of WebControl.</p>"
                                                    +"<p>WebControl:"+str(float(self.data.stockFirmwareVersion))+", Controller:"+str(tmpVersion)+"</p>"
                                                    )
                # 50 <= x < 100 implies holey calibration firmware
                if tmpVersion >= 50 and tmpVersion < 100:
                    if self.data.holeyFirmwareVersion is not None:
                        if tmpVersion < float(self.data.holeyFirmwareVersion):
                            self.data.ui_queue1.put("Alert", "Alert",
                                                        "<p>Warning, you are running a Holey Calibration firmware that is not up to date.  This version may not work correctly with this version of WebControl.</p>"
                                                        + "</p><p>Please, click Actions->Upgrade Holey Firmware to update the controller to the latest WebControl-compatible code.</p>"
                                                        +"<p>WebControl:"+str(float(self.data.holeyFirmwareVersion))+", Controller:"+str(tmpVersion)+"</p>"
                                                        )
                        elif tmpVersion > float(self.data.holeyFirmwareVersion):
                            self.data.ui_queue1.put("Alert", "Alert",
                                                    "<p>Warning, you are running a Holey Calibration firmware that is newer than what is included in WebControl.  This version may not work correctly with this version of WebControl.</p>"
                                                    "<p>WebControl:"+str(float(self.data.holeyFirmwareVersion))+", Controller:"+str(tmpVersion)+"</p>"
                                                    )
                # x >= 100 implies optical calibration firmware
                if tmpVersion >= 100:
                    if self.data.customFirmwareVersion is not None:
                        if tmpVersion < float(self.data.customFirmwareVersion):
                            self.data.ui_queue1.put("Alert", "Alert",
                                                    "<p>Warning, you are running a custom firmware that is not"
                                                    + " up to date.  This version may not work correctly with"
                                                    + " this version of WebControl.</p>"
                                                    + "</p><p>Please, click Actions->Upgrade Custom Firmware to"
                                                    + " update the controller to the latest"
                                                    + " WebControl-compatible code.</p>"
                                                    + "<p>WebControl:"
                                                    + str(float(self.data.customFirmwareVersion))
                                                    + ", Controller:"+str(tmpVersion)+"</p>"
                                                    )
                        elif tmpVersion > float(self.data.customFirmwareVersion):
                            self.data.ui_queue1.put("Alert", "Alert",
                                                    "<p>Warning, you are running a custom firmware that is "
                                                    + "newer than what is included in WebControl.  This version"
                                                    + " may not work correctly with this version of WebControl."
                                                    + "</p><p>WebControl:"
                                                    + str(float(self.data.customFirmwareVersion))
                                                    + ", Controller:" + str(tmpVersion)+"</p>"
                                                    )

            elif message == "ok\r\n":
                pass  # displaying all the 'ok' messages clutters up the display

            ###
            ### Velocity PID Testing Processing
            ### Watch for messages and log data
            ###
            elif message[0:26] == "--PID Velocity Test Stop--":
                self.data.actions.velocityPIDTestRun("stop", "")
            elif self.data.inPIDVelocityTest:
                self.data.actions.velocityPIDTestRun("running", message)
            elif message[0:27] == "--PID Velocity Test Start--":
                self.data.actions.velocityPIDTestRun("start", "")
            ###
            ### Positional PID Testing Processing
            ### Watch for messages and log data
            ###
            elif message[0:26] == "--PID Position Test Stop--":
                self.data.actions.positionPIDTestRun("stop", "")
            elif self.data.inPIDPositionTest:
                self.data.actions.positionPIDTestRun("running", message)
            elif message[0:27] == "--PID Position Test Start--":
                self.data.actions.positionPIDTestRun("start", "")

            else:
                # Just send it to the UIProcessor for processing.
                self.data.ui_controller_queue.put(message)
