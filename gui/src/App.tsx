import { Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Sites from "./pages/Sites";
import PHP from "./pages/PHP";
import Database from "./pages/Database";
import SSL from "./pages/SSL";
import Nginx from "./pages/Nginx";
import Logs from "./pages/Logs";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Sites />} />
        <Route path="/php" element={<PHP />} />
        <Route path="/nginx" element={<Nginx />} />
        <Route path="/database" element={<Database />} />
        <Route path="/ssl" element={<SSL />} />
        <Route path="/logs" element={<Logs />} />
      </Route>
    </Routes>
  );
}
