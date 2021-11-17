import "./seedphrase.scss";
import React, { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { sendMessagePromise } from "../../redux/common";

export default function SeedPhrase() {
  const [seedWords, setSeedWords] = useState("");
  const [showSeedWords, setShowSeedWords] = useState(false);
  useEffect(() => {
    // callback
    sendMessagePromise({ action: "tari-get-seedwords" }).then((value) => {
      console.log(value);
      setSeedWords(value.seedWords);
    });
  }, []);
  return (
    <div className="screen">
      <div className="caption">SeedPhrase</div>
      <div className="text">You will not see these again.</div>
      <div className="seed-words-wrapper">
        <div className="seed-words">
          {seedWords}
          <div
            className={`protection ${showSeedWords ? "hide" : ""}`}
            onClick={() => setShowSeedWords(true)}
          >
            <div>Click to reveal your seed words</div>
          </div>
        </div>
      </div>
      <Link
        to="../confirm-seed-words"
        className={`button ${showSeedWords ? "" : "disabled-button"}`}
        state={{ seedWords }}
      >
        Next
      </Link>
    </div>
  );
}
