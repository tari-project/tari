// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import "./app.scss";
import React, { useEffect } from "react";
import { Navigate, Route, Routes } from "react-router";
import { useDispatch, useSelector } from "react-redux";
import Onboarding from "./onboarding/Onboarding";
import { HashRouter } from "react-router-dom";
import Popup from "./popup/Popup";
import { getAccounts, getAccountsStatusSelector } from "./redux/accountSlice";
import Connecting from "./connecting/Connecting";

export default function App() {
  const accountsStatus = useSelector(getAccountsStatusSelector);

  const dispatch = useDispatch();
  useEffect(() => {
    dispatch(getAccounts());
  }, [dispatch]);
  console.log("accountsStatus", accountsStatus);
  if (accountsStatus === "empty") {
    if (!window.location.href.includes("#/onboarding")) {
      window.open("#/onboarding");
      window.close();
      return <div></div>;
    }
  }
  return (
    <div className="main">
      <HashRouter>
        <Routes>
          <Route path="/onboarding/*" element={<Onboarding />} />
          <Route path="/connecting/:site/:id" element={<Connecting />} />
          <Route path="/popup/*" element={<Popup />} />
          <Route path="" element={<Navigate replace to="popup" />} />
        </Routes>
      </HashRouter>
    </div>
  );
}
