// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import React from "react";
import "./App.css";
import { Route, Routes } from "react-router";
import Home from "./Scenes/Home/Home";
import Connect from "./Scenes/Connect/Connect";

function App() {
  return (
    <Routes>
      <Route path="/" element={<Home />} />
      <Route path="/connect" element={<Connect />} />
    </Routes>
  );
}

export default App;
