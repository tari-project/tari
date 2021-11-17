import React, { useState } from "react";
import { useDispatch } from "react-redux";
import { Link } from "react-router-dom";
import { login } from "../../redux/loginSlice";

export default function CreateWallet() {
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const dispatch = useDispatch();

  const onPasswordChange = (e) => {
    setPassword(e.target.value);
  };

  const onConfirmPasswordChange = (e) => {
    setConfirmPassword(e.target.value);
  };

  const checkPasswords = () =>
    password === confirmPassword && password.length >= 6;

  const createWallet = () => {
    console.log("dispatching login");
    const username = "username";
    dispatch(login({ username, password }));
  };

  return (
    <div className="screen">
      <div className="caption">Create Password</div>
      New password (....)
      <input
        name="password"
        value={password}
        type="password"
        onChange={onPasswordChange}
      ></input>
      Confirm password
      <input
        name="confirm-password"
        value={confirmPassword}
        type="password"
        onChange={onConfirmPasswordChange}
      ></input>
      <Link
        to="../seed-phrase"
        className={`button ${checkPasswords() ? "" : "disabled-button"}`}
        onClick={createWallet}
      >
        Create
      </Link>
    </div>
  );
}
