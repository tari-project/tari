import "./confirmseedwords.scss";
import React, { useState } from "react";
import { Link } from "react-router-dom";
import { useLocation } from "react-router";

export default function ConfirmSeedWords() {
  const [wordsOrder, setWordsOrder] = useState([]);
  const location = useLocation();
  const seedWords = location.state.seedWords.split(" ");
  const alphabeticalSeedWords = [...seedWords];
  alphabeticalSeedWords.sort();
  const onWordClick = (word) => {
    if (wordsOrder.includes(word)) {
      // remove
      setWordsOrder(wordsOrder.filter((value) => value !== word));
    } else {
      // add
      setWordsOrder([...wordsOrder, word]);
    }
  };
  const checkSeedWords = () =>
    JSON.stringify(seedWords) === JSON.stringify(wordsOrder);

  return (
    <div className="screen">
      <div className="caption">ConfirmSeedWords</div>
      <div className="ordered">
        <div className="ordered-items">
          {wordsOrder.map((word) => (
            <div key={word} className="seed-word">
              {word}
            </div>
          ))}
        </div>
      </div>
      <div className="unordered">
        {alphabeticalSeedWords.map((word) => (
          <div
            key={word}
            className={`seed-word seed-word-button ${
              wordsOrder.includes(word) ? "seed-word-button-pressed" : ""
            }`}
            onClick={() => onWordClick(word)}
          >
            {word}
          </div>
        ))}
      </div>
      <Link
        to="../complete"
        className={`button ${checkSeedWords() || 1 ? "" : "disabled-button"}`}
      >
        Next
      </Link>
    </div>
  );
}
