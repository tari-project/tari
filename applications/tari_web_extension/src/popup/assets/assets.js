import "./assets.css";
import { useEffect } from "react";
import Asset from "./components/asset";
import { useSelector, useDispatch } from "react-redux";
import {
  getAssets,
  getSelectedAsset,
  loadAssets,
  selectAsset,
} from "../../redux/assetsSlice";

export default function Assets() {
  const assets = useSelector(getAssets);
  const selected = useSelector(getSelectedAsset);
  const dispatch = useDispatch();
  useEffect(() => {
    dispatch(loadAssets());
  }, [dispatch]);

  function select(name) {
    dispatch(selectAsset(name));
  }

  return (
    <div className="assets">
      <div className="title">Assets:</div>
      {assets.map((name) => (
        <Asset
          key={name}
          name={name}
          selected={name === selected}
          onClick={() => select(name)}
        />
      ))}
    </div>
  );
}
