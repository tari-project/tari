import "./popup.scss";
import { Navigate, Route, Routes } from "react-router";
import Assets from "./assets/assets";

function Popup() {
  return (
    <div className="popup">
      <Routes>
        <Route path="assets" element={<Assets />} />
        <Route path="" element={<Navigate replace to="assets" />} />
      </Routes>
    </div>
  );
}

export default Popup;
