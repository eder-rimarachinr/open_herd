import { Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Sites    from "./pages/Sites";
import PHP      from "./pages/PHP";
import SSL      from "./pages/SSL";
import Nginx    from "./pages/Nginx";
import Logs     from "./pages/Logs";
import Database from "./pages/Database";
import Settings from "./pages/Settings";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/"         element={<Sites />} />
        <Route path="/php"      element={<PHP />} />
        <Route path="/nginx"    element={<Nginx />} />
        <Route path="/ssl"      element={<SSL />} />
        <Route path="/logs"     element={<Logs />} />
        <Route path="/database" element={<Database />} />
        <Route path="/settings" element={<Settings />} />
      </Route>
    </Routes>
  );
}
