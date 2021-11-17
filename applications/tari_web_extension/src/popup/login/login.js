import "./login.css";
import { useState } from "react";
import { useDispatch } from "react-redux";
import { login } from "../../redux/loginSlice";

export default function Login() {
  const [username, setUsername] = useState(() => {
    let saved = localStorage.getItem("tari-username");
    return saved || "";
  });
  const [password, setPassword] = useState("");
  const dispatch = useDispatch();

  function handleSubmit(event) {
    dispatch(login({ username, password }));
  }

  return (
    <div className="login">
      <div className="logo">
        <img src="/tari-logo.svg" alt="Tari logo" />
      </div>
      <div className="form">
        <div>Username</div>
        <label className="editbox">
          <input
            type="text"
            name="username"
            value={username}
            onChange={(event) => setUsername(event.target.value)}
          />
        </label>
        <div>Password</div>
        <label className="editbox">
          <input
            type="password"
            name="password"
            value={username}
            onChange={(event) => setPassword(event.target.value)}
          />
        </label>
      </div>
      <div onClick={handleSubmit} className="submit">
        Login
      </div>
    </div>
  );
}
