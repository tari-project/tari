import "./importwallet.scss";
import React, { useState } from "react";
import { Link } from "react-router-dom";

export default function ImportWallet() {
  const [seedWords, setSeedWords] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const onSeedPhraseChange = (e) => {
    setSeedWords(e.target.value);
  };
  const onPasswordChange = (e) => {
    setPassword(e.target.value);
  };

  const onConfirmPasswordChange = (e) => {
    setConfirmPassword(e.target.value);
  };

  const checkSeedWords = () => seedWords.split(" ").length === 24;

  const checkPasswords = () =>
    password === confirmPassword && password.length >= 6;

  const checkForm = () => checkSeedWords() && checkPasswords();

  return (
    <div className="screen">
      <div className="caption">ImportWallet</div>
      <textarea
        className="seedwords"
        value={seedWords}
        onChange={onSeedPhraseChange}
      />
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
        to="../complete"
        className={`button ${checkForm() ? "" : "disabled-button"}`}
      >
        Next
      </Link>
    </div>
  );
}
