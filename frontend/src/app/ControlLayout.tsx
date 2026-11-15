import { Navigate, Route, Routes, matchPath, useLocation, useParams } from "react-router";
import { MainPage } from "../features/main";
import { Dialog } from "./Dialog";
import { ModalContext, useModalClose } from "./modalHistory";
import { legacyRedirects, modalRoutes } from "./modals";
import type { ModalRoute } from "./modals";
import { TopBar } from "./TopBar";

const ModalFrame = ({ route, close }: { route: ModalRoute; close: () => void }) => {
  const params = useParams();
  const title = typeof route.title === "function" ? route.title(params) : route.title;
  return (
    <ModalContext value={close}>
      <Dialog title={title} onClose={close} size={route.size}>
        <route.Component />
      </Dialog>
    </ModalContext>
  );
};

const NotFound = ({ close }: { close: () => void }) => (
  <Dialog title="Not found" onClose={close}>
    <p>There is no screen at this address.</p>
  </Dialog>
);

const HelpRedirect = () => <Navigate to={`/help/${useParams()["*"] ?? ""}`} replace />;

const LEGACY_PATHS = [...Object.keys(legacyRedirects), "system/help/*"];

/** The control screen, with any route modal over it: /files/open is the control screen plus Open G-Code. */
export const ControlLayout = () => {
  const { pathname } = useLocation();
  const legacy = LEGACY_PATHS.some((path) => matchPath(`/${path}`, pathname));
  const modalOpen = pathname !== "/" && !legacy;
  const close = useModalClose(modalOpen);

  return (
    <div className="shell">
      <div className="shell-page" aria-hidden={modalOpen || undefined}>
        <TopBar />
        <main className="content">
          <MainPage />
        </main>
      </div>
      <Routes>
        <Route index element={null} />
        {modalRoutes.map((route) => (
          <Route key={route.path} path={route.path} element={<ModalFrame route={route} close={close} />} />
        ))}
        {Object.entries(legacyRedirects).map(([from, to]) => (
          <Route key={from} path={from} element={<Navigate to={to} replace />} />
        ))}
        <Route path="system/help/*" element={<HelpRedirect />} />
        <Route path="*" element={<NotFound close={close} />} />
      </Routes>
    </div>
  );
};
