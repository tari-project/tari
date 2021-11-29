import "./popup.scss";
import { Navigate, Route, Routes } from "react-router";
import Home from "./Home/Home";

function Popup() {
  return (
    <div className="popup">
      <Routes>
        <Route path="home" element={<Home />} />
        <Route path="" element={<Navigate replace to="home" />} />
      </Routes>
    </div>
  );
}

export default Popup;
