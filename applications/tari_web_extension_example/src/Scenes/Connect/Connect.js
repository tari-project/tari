// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import "./connect.scss";
import React, { useEffect } from "react";
import {
  metamaskCheckConnection,
  metamaskStatusSelector,
  metamaskAccountsSelector,
} from "../../features/metamask/metamaskReducer";
import { useDispatch, useSelector } from "react-redux";
import {
  tariAccountsSelector,
  tariCheckConnection,
  tariStatusSelector,
} from "../../features/tari/tariReducer";

export default function Connect() {
  const dispatch = useDispatch();
  useEffect(() => {
    dispatch(metamaskCheckConnection());
    dispatch(tariCheckConnection());
  }, [dispatch]);
  const metamaskStatus = useSelector(metamaskStatusSelector);
  const metamaskAccounts = useSelector(metamaskAccountsSelector);
  const tariStatus = useSelector(tariStatusSelector);
  const tariAccounts = useSelector(tariAccountsSelector);

  const connect_metamask = async () => {
    console.log(
      await window.ethereum.request({ method: "eth_requestAccounts" })
    );
  };

  const connect_tari = async () => {
    // This should also come via listener
    console.log(await window.tari.connect());
  };

  const metamask = () => {
    switch (metamaskStatus) {
      case "connected":
        return <div>{metamaskAccounts}</div>;
      case "installed":
        return (
          <div className="button" onClick={connect_metamask}>
            Connect to metamask
          </div>
        );
      case "not installed":
        return (
          <a
            className="button"
            target="_blank"
            rel="noreferrer"
            href="https://metamask.io/"
          >
            Install Metamask
          </a>
        );
      default:
        return <div>Please wait...</div>;
    }
  };

  const tari = () => {
    switch (tariStatus) {
      case "connected":
        return <div>{tariAccounts}</div>;
      case "installed":
        return (
          <div className="button" onClick={connect_tari}>
            Connect to tari
          </div>
        );
      case "not installed":
        return (
          <a className="button" target="_blank" rel="noreferrer" href="#">
            Install Tari
          </a>
        );
      default:
        return <div>Please wait...</div>;
    }
  };

  return (
    <div className="connect">
      <h1>Extensions</h1>
      {metamask()}
      {tari()}
    </div>
  );
}
