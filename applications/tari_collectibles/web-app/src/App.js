//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

import Dashboard from "./Dashboard";
import {
  Route,
  BrowserRouter as Router,
  Switch,
  Redirect,
  Link as RouterLink,
} from "react-router-dom";
import { createTheme } from "@mui/material/styles";
import {
  Alert,
  Box,
  CssBaseline,
  Divider,
  Drawer,
  IconButton,
  List,
  ListItem,
  ListItemIcon,
  ListItemText,
  ListSubheader,
  Toolbar,
} from "@mui/material";
import DashboardIcon from "@mui/icons-material/Dashboard";
import CreateIcon from "@mui/icons-material/Create";
import AddIcon from "@mui/icons-material/Add";
import AppsIcon from "@mui/icons-material/Apps";
import { ThemeProvider } from "@emotion/react";
import Create from "./Create";
import { deepPurple, pink } from "@mui/material/colors";
import "./App.css";
import * as React from "react";
import PropTypes from "prop-types";
import Manage from "./Manage";
import AssetManager from "./AssetManager";
import AccountDashboard from "./AccountDashboard";
import NewAccount from "./NewAccount";
import Setup, { UnlockWallet } from "./Setup";
import { useEffect, useState } from "react";
import binding from "./binding";
import { Spinner } from "./components";
import { listen } from "@tauri-apps/api/event";

const mdTheme = createTheme({
  palette: {
    mode: "dark",
    primary: deepPurple,
    secondary: pink,
  },
});

function IconButtonLink(props) {
  const { icon, to } = props;
  const renderLink = React.useMemo(
    () =>
      React.forwardRef(function Link(itemProps, ref) {
        return <RouterLink to={to} ref={ref} {...itemProps} role={undefined} />;
      }),
    [to]
  );

  return (
    <IconButton edge="end" aria-label="add" component={renderLink}>
      {icon}
    </IconButton>
  );
}

IconButtonLink.propTypes = {
  icon: PropTypes.element.isRequired,
  to: PropTypes.string.isRequired,
};

function ListItemLink(props) {
  const { icon, primary, to } = props;

  const renderLink = React.useMemo(
    () =>
      React.forwardRef(function Link(itemProps, ref) {
        return <RouterLink to={to} ref={ref} {...itemProps} role={undefined} />;
      }),
    [to]
  );

  return (
    <ListItem button component={renderLink}>
      {icon ? <ListItemIcon>{icon}</ListItemIcon> : null}
      <ListItemText primary={primary} />
    </ListItem>
  );
}

ListItemLink.propTypes = {
  icon: PropTypes.element,
  primary: PropTypes.string.isRequired,
  to: PropTypes.string.isRequired,
};

const AccountsMenu = (props) => {
  const [accounts, setAccounts] = useState([]);
  const [error, setError] = useState("");

  useEffect(async () => {
    console.log("refreshing accounts");
    setError("");
    await binding
      .command_asset_wallets_list()
      .then((accounts) => {
        console.log("accounts", accounts);
        setAccounts(accounts);
      })
      .catch((e) => {
        // todo error handling
        console.error("accounts_list error:", e);
        setError(e.message);
      });

    await listen("asset_wallets::updated", (event) => {
      console.log("accounts have changed");
      setError("");
      binding
        .command_asset_wallets_list()
        .then((accounts) => {
          console.log("accounts", accounts);
          setAccounts(accounts);
        })
        .catch((e) => {
          // todo error handling
          console.error("accounts_list error:", e);
          setError(e.message);
        });
    });
  }, [props.walletId]);

  // todo: hide accounts when not authenticated
  return (
    <div>
      <ListSubheader>
        <ListItem
          component="div"
          disableGutters={true}
          secondaryAction={
            <IconButtonLink icon={<AddIcon />} to="/accounts/new" />
          }
        >
          My Assets
        </ListItem>
      </ListSubheader>
      <List>
        {accounts.map((item) => {
          return (
            <ListItemLink
              key={item.name}
              primary={item.name || item.assetPublicKey}
              to={`/accounts/${item.asset_public_key}`}
            />
          );
        })}
      </List>
      {error ? (
        <Alert severity="error" onClick={() => setError(null)}>
          {error}
        </Alert>
      ) : (
        ""
      )}
    </div>
  );
};

AccountsMenu.propTypes = {
  walletId: PropTypes.string,
};

// only allow access to a Protected Route if the wallet is unlocked
const ProtectedRoute = ({ authenticated, path, children }) => {
  if (!authenticated) return <Redirect to="/unlock" />;

  return <Route path={path}>{children}</Route>;
};

ProtectedRoute.propTypes = {
  authenticated: PropTypes.bool.isRequired,
  path: PropTypes.string.isRequired,
  children: PropTypes.node.isRequired,
};

function App() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [authenticated, setAuthenticated] = useState(false);
  const [walletId, setWalletId] = useState("");
  const setPassword = useState("")[1];

  // todo: screen lock after x mins no activity

  // ensure db created or open before other components try to make db calls
  useEffect(() => {
    binding
      .command_create_db()
      .then((r) => setLoading(false))
      .catch((e) => {
        setLoading(false);
        setError(e);
      });
  }, []);
  if (loading) return <Spinner />;
  if (error) return <Alert severity="error">{error.toString()}</Alert>;

  return (
    <div className="App">
      <Router>
        <ThemeProvider theme={mdTheme}>
          <Box sx={{ display: "flex" }}>
            <CssBaseline />

            <Drawer variant="permanent" hidden={!walletId}>
              <RouterLink to="/">
                <Toolbar sx={{ display: "flex", color: "white" }}>
                  Tari Collectibles
                </Toolbar>
              </RouterLink>
              <List>
                <ListItemLink
                  primary="Dashboard"
                  to="/dashboard"
                  icon={<DashboardIcon />}
                />
                <Divider />
                <AccountsMenu walletId={walletId} />
                <ListSubheader>Issued Assets</ListSubheader>
                <ListItemLink
                  primary="Manage"
                  to="/manage"
                  icon={<AppsIcon />}
                />
                <ListItemLink
                  primary="Create"
                  to="/create"
                  icon={<CreateIcon />}
                />
                {/*<Divider></Divider>*/}
                {/*<ListSubheader>My Wallet</ListSubheader>*/}
                {/*<ListItemLink*/}
                {/*  primary="Main"*/}
                {/*  to={`/wallets/${walletId}`}*/}
                {/*  icon={<AccountBalanceWalletIcon />}*/}
                {/*/>*/}
              </List>
            </Drawer>
            <Box
              component="main"
              sx={{ flexGrow: 1, height: "100vh", overflow: "auto" }}
            >
              <Switch>
                <ProtectedRoute
                  path="/accounts/new"
                  authenticated={authenticated}
                >
                  <NewAccount />
                </ProtectedRoute>
                <ProtectedRoute
                  path="/accounts/:assetPubKey"
                  authenticated={authenticated}
                >
                  <AccountDashboard />
                </ProtectedRoute>
                <ProtectedRoute path="/create" authenticated={authenticated}>
                  <Create />
                </ProtectedRoute>
                <ProtectedRoute path="/manage" authenticated={authenticated}>
                  <Manage />
                </ProtectedRoute>
                <ProtectedRoute
                  path="/assets/manage/:assetPubKey"
                  authenticated={authenticated}
                >
                  <AssetManager />
                </ProtectedRoute>

                <Route path="/wallets/:id">
                  <UnlockWallet
                    setAuthenticated={(id, password) => {
                      setWalletId(id);
                      setPassword(password);
                      setAuthenticated(true);
                    }}
                  />
                </Route>
                <Route path="/unlock">
                  <Setup
                    setAuthenticated={(id, password) => {
                      setWalletId(id);
                      setPassword(password);
                      setAuthenticated(true);
                    }}
                  />
                </Route>
                <ProtectedRoute path="/" authenticated={authenticated}>
                  <Dashboard />
                </ProtectedRoute>
              </Switch>
            </Box>
          </Box>
        </ThemeProvider>
      </Router>
    </div>
  );
}

export default App;
