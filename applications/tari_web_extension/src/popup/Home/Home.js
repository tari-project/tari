// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import "./home.scss";
import { useEffect } from "react";
import Asset from "./components/Asset";
import { useSelector, useDispatch } from "react-redux";
import {
  getAssets,
  getSelectedAsset,
  loadAssets,
  selectAsset,
} from "../../redux/assetsSlice";
import { getAccountsSelector } from "../../redux/accountSlice";
import { getConnectedSites, getSitesSelector } from "../../redux/sitesSlice";
import Account from "./components/Account";
import Site from "./components/Site";

export default function Assets() {
  const assets = useSelector(getAssets);
  const selected = useSelector(getSelectedAsset);
  const accounts = useSelector(getAccountsSelector);
  const sites = useSelector(getSitesSelector);
  const dispatch = useDispatch();
  useEffect(() => {
    dispatch(loadAssets());
    dispatch(getConnectedSites());
  }, [dispatch]);

  function select(name) {
    dispatch(selectAsset(name));
  }

  const onAccountClick = (account) => {
    console.log(account);
  };

  const onSiteClick = (site) => {
    console.log(site);
  };

  return (
    <div className="home">
      <div className="title">Assets:</div>
      {assets.map((name) => (
        <Asset
          key={name}
          name={name}
          selected={name === selected}
          onClick={() => select(name)}
        />
      ))}
      <div className="title">Accounts:</div>
      {accounts.map((account) => (
        <Account
          key={account}
          name={account}
          onClick={() => onAccountClick(account)}
        />
      ))}
      <div className="title">Connected Sites:</div>
      {Object.keys(sites).map((site) => (
        <Site key={site} name={site} onClick={() => onSiteClick(site)} />
      ))}
    </div>
  );
}
