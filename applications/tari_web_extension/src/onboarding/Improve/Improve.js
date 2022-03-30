// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import React from "react";
import { useLocation } from "react-router";
import { Link } from "react-router-dom";

export default function Improve() {
  const location = useLocation();
  const { next } = location.state;
  return (
    <div className="screen">
      <div className="caption">Help us improve</div>
      <Link to={next} className="button">
        I Agree
      </Link>
    </div>
  );
}
