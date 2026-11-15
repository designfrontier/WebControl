import { Route, Routes, useMatch } from "react-router";
import { CalibrationPage } from "../features/calibration";
import { ServerDialogs } from "../features/main";
import { LogsPage } from "../features/system";
import { ControlLayout } from "./ControlLayout";
import { ThemeProvider } from "./theme";

export const App = () => {
  const onLogs = useMatch("/logs") !== null;
  return (
    <ThemeProvider>
      <Routes>
        <Route path="logs" element={<LogsPage />} />
        <Route path="calibration/*" element={<CalibrationPage />} />
        <Route path="*" element={<ControlLayout />} />
      </Routes>
      {!onLogs && <ServerDialogs />}
    </ThemeProvider>
  );
};
