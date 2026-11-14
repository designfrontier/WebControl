import { Link } from "react-router";
import { Screen } from "./Screen";

const Figure = ({ file, caption }: { file: string; caption: string }) => (
  <figure className="setup-figure wide">
    <img src={`/static/images/gettingStarted/${encodeURIComponent(file)}`} alt={caption} loading="lazy" />
    <figcaption>{caption}</figcaption>
  </figure>
);

export const GettingStartedPage = () => (
  <Screen title="Getting started">
    <h3>Coming from GroundControl</h3>
    <p>
      Import your groundcontrol.ini file before anything else (<Link to="/settings">Settings</Link>). If the COM port is
      still the same, WebControl should then talk to the controller straight away.
    </p>

    <h3>Starting anew</h3>
    <ol className="steps">
      <li>
        <h4>Set the COM port</h4>
        <p>
          In <Link to="/settings">Settings</Link>, under Maslow Settings, pick the COM port of the Arduino controller.
          Refresh the list to find ports; with Bluetooth on this can take a few seconds. Save, and controller messages
          should start to appear.
        </p>
        <Figure file="comPort.png" caption="COM port setting" />
      </li>
      <li>
        <h4>Set the distance to extend the chains</h4>
        <p>
          With a top beam longer than 10 feet, change Extend Chain Distance under Advanced Settings (2032 for a 12-foot
          beam). Too little and the chains will not meet the sled; too much and the sled may end up on the floor. Keep
          it a multiple of 6.35 mm.
        </p>
        <Figure file="extendChainDistance.png" caption="Extend chain distance setting" />
      </li>
      <li>
        <h4>Quick configure</h4>
        <p>
          Enter your frame measurements in <Link to="/machine/quick-configure">Quick configure</Link> and save. This
          tells the controller enough to calibrate.
        </p>
      </li>
      <li>
        <h4>Set the sprockets vertical</h4>
        <p>
          Open <Link to="/machine/sprockets">Sprockets and chains</Link>:
        </p>
        <ol type="a">
          <li>
            Turn one tooth of each sprocket as precisely vertical as you can, then press Define zero. That tells the
            controller the chain length is zero.
            <Figure file="Sprocket at 12-00.png" caption="Sprocket with a vertical tooth" />
          </li>
          <li>
            Put the chains on the sprockets with the first link resting on the top tooth. Where they hang depends on
            whether you built the frame for chains off the top or off the bottom.
            <Figure file="chainOffSprocketsBottom.png" caption="Chains off the bottom" />
            <Figure file="chainOffSprocketsTop.png" caption="Chains off the top" />
          </li>
          <li>
            Extend the left chain.{" "}
            <strong className="warning">
              The chain can wrap around the sprocket here: hold the end and keep tension on it as it feeds off.
            </strong>
          </li>
          <li>
            Extend the right chain. <strong className="warning">Same warning as above.</strong>
          </li>
          <li>Connect the sled to the extended chains.</li>
          <li>
            Mark the chain link sitting on the top tooth of each sprocket. If you ever need to{" "}
            <Link to="/machine/reset-chains">reset the chains</Link>, that link goes back on the vertical tooth.
          </li>
        </ol>
      </li>
      <li>
        <h4>Calibrate</h4>
        <p>
          In <Link to="/calibration">Calibration</Link>, use triangular calibration with stock firmware or holey
          calibration with holey firmware. Cut the calibration pattern, enter the measurements, calculate, and accept
          the results if they look right. You are calibrated.
        </p>
      </li>
    </ol>
  </Screen>
);
