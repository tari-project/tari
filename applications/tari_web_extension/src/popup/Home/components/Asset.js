import "./asset.scss";

export default function Asset({ name, selected, onClick }) {
  return (
    <div className={`asset ${selected && "selected"}`} onClick={onClick}>
      {name}
    </div>
  );
}
