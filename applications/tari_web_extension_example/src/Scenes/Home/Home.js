import "./home.scss";
import React from "react";
import { Link } from "react-router-dom";

export default function Home() {
  return (
    <div className="home">
      <h1>HOME</h1>
      <Link to="/connect">Connect</Link>
    </div>
  );
}
