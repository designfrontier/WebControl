import { Route, Routes } from "react-router";
import { Layout } from "./Layout";
import { sections } from "./sections";

const NotFound = () => (
  <section>
    <h1>Not found</h1>
    <p>There is no screen at this address.</p>
  </section>
);

export const App = () => (
  <Routes>
    <Route element={<Layout />}>
      {sections.map(({ path, label, Component }) =>
        path === "" ? (
          <Route key={label} index element={<Component />} />
        ) : (
          <Route key={label} path={`${path}/*`} element={<Component />} />
        ),
      )}
      <Route path="*" element={<NotFound />} />
    </Route>
  </Routes>
);
