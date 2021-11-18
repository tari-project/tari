import React from "react";
import { Link } from "react-router-dom";

export default function Welcome() {
  return (
    <div className="screen">
      <div className="caption">Welcome to Tari Web Extension</div>
      <p className="text">
        Lorem Ipsum is simply dummy text of the printing and typesetting
        industry.
      </p>
      <p className="text">
        Lorem Ipsum has been the industry's standard dummy text ever since the
        1500s, when an unknown printer took a galley of type and scrambled it to
        make a type specimen book
      </p>
      <Link to="../setup-wallet" className="button">
        Next
      </Link>
    </div>
  );
}
