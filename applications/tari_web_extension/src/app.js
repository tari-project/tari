import "./app.scss";
import React, { useEffect } from "react";
import { Navigate, Route, Routes } from "react-router";
import { useDispatch, useSelector } from "react-redux";
import {
  getCredentials,
  getCredentialsCalled,
  refreshLogin,
} from "./redux/loginSlice";
import Onboarding from "./onboarding/Onboarding";
import { HashRouter } from "react-router-dom";
import Popup from "./popup/Popup";

export default function App() {
  const credentials = useSelector(getCredentials);
  const credentialsCalled = useSelector(getCredentialsCalled);
  const dispatch = useDispatch();
  useEffect(() => {
    dispatch(refreshLogin());
  }, [dispatch]);
  if (!credentials) {
    if (!window.location.href.includes("#/onboarding") && credentialsCalled) {
      window.open("#/onboarding");
    }
  }
  return (
    <div className="main">
      <HashRouter>
        <Routes>
          <Route path="/onboarding/*" element={<Onboarding />} />
          <Route path="/popup/*" element={<Popup />} />
          <Route path="" element={<Navigate replace to="popup" />} />
        </Routes>
      </HashRouter>
    </div>
  );
}
