import "./site.scss";

export default function Site({ name, onClick }) {
  return (
    <div className={`site`} onClick={onClick}>
      {name}
    </div>
  );
}
