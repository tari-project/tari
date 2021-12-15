import "./account.scss";

export default function Account({ name, onClick }) {
  return (
    <div className={`account`} onClick={onClick}>
      {name}
    </div>
  );
}
