import "./connecting.scss";
import React from "react";
import { useParams } from "react-router";
import { sendMessagePromise } from "../redux/common";
import { useDispatch } from "react-redux";
import { connectSite } from "../redux/sitesSlice";

export default function Connecting() {
  const params = useParams();
  const dispatch = useDispatch();
  const site = Buffer.from(params.site, "base64").toString();
  const callback_id = params.id;
  const onCancel = () => {
    window.close();
  };
  const onConfirm = () => {
    dispatch(connectSite({ site, callback_id }));
    window.close();
  };

  return (
    <div className="screen">
      <div className="caption">You are about to give access to {site}</div>
      <div className="row">
        <div className="button" onClick={onCancel}>
          Cancel
        </div>
        <div className="button" onClick={onConfirm}>
          Confirm
        </div>
      </div>
    </div>
  );
}
