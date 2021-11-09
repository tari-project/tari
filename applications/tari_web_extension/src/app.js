import "./app.css";
import React, { useEffect } from "react";
import { MemoryRouter, Navigate, Route, Routes } from "react-router";
import Assets from "./screens/assets/assets";
import Login from "./screens/login/login";
import { useDispatch, useSelector } from "react-redux";
import { getCredentials, refreshLogin } from "./redux/loginSlice";

export default function App() {
  const credentials = useSelector(getCredentials);
  const dispatch = useDispatch();
  useEffect(() => {
    dispatch(refreshLogin());
  }, [dispatch]);
  console.log("he?", credentials);
  return (
    <div className="main">
      <MemoryRouter>
        <Routes>
          <Route path="/assets" element={<Assets />} />
          <Route
            path="*"
            element={credentials ? <Navigate to="/assets" /> : <Login />}
          />
        </Routes>
      </MemoryRouter>
    </div>
  );
}
