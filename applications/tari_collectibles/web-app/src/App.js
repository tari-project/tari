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
  Link as RouterLink,
} from "react-router-dom";
import { createTheme } from "@mui/material/styles";
import {
  AppBar,
  Box,
  CssBaseline, Divider,
  Drawer, IconButton,
  List,
  ListItem,
  ListItemIcon,
  ListItemText, ListSubheader,
  Toolbar,
  Typography,
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

const mdTheme = createTheme({
  palette: {
    mode: "dark",
    primary: deepPurple,
    secondary: pink,
  },
});

function IconButtonLink(props) {
  const { icon, to} = props;
  const renderLink = React.useMemo(
      () => React.forwardRef(function Link(itemProps, ref) {
        return <RouterLink to={to} ref={ref} {...itemProps} role={undefined} />;
      }),
        [to]
  )

  return (
      <IconButton edge="end" aria-label="add" component={renderLink} >{icon}</IconButton>
  );
}

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
    <li>
      <ListItem button component={renderLink}>
        {icon ? <ListItemIcon>{icon}</ListItemIcon> : null}
        <ListItemText primary={primary} />
      </ListItem>
    </li>
  );
}

ListItemLink.propTypes = {
  icon: PropTypes.element,
  primary: PropTypes.string.isRequired,
  to: PropTypes.string.isRequired,
};

function App() {
  return (
    <div className="App">
      <Router>
        <ThemeProvider theme={mdTheme}>
          <Box sx={{ display: "flex" }}>
            <CssBaseline />
            <AppBar position="absolute">
              <Toolbar>
                <Typography component="h1">Hello world</Typography>
              </Toolbar>
            </AppBar>
            <Drawer variant="permanent">
              <Toolbar sx={{ display: "flex" }}>Tari Collectibles</Toolbar>
              <List>
                <ListItemLink
                  primary="Dashboard"
                  to="/"
                  icon={<DashboardIcon />}
                />
                <Divider></Divider>
                <ListSubheader ><ListItem disableGutters={true} secondaryAction={
                  <IconButtonLink icon={<AddIcon />} to="/accounts/new">
                  </IconButtonLink>
                }>My Assets</ListItem></ListSubheader>
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
              </List>
            </Drawer>
            <Box
              component="main"
              sx={{ flexGrow: 1, height: "100vh", overflow: "auto" }}
            >
              <Switch>
                <Route path="/accounts/new" >
                  <NewAccount />
                </Route>
                <Route path="/accounts/:assetPubKey">
                  <AccountDashboard />
                </Route>


                <Route path="/create">
                  <Create />
                </Route>
                <Route path="/manage">
                  <Manage />
                </Route>
                <Route path="/assets/manage/:assetPubKey">
                  <AssetManager />
                </Route>
                <Route path="/">
                  <Dashboard />
                </Route>
              </Switch>
            </Box>
          </Box>
        </ThemeProvider>
      </Router>
    </div>
  );
}

export default App;
