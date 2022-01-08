import "./onboarding.scss";
import { Navigate, Route, Routes } from "react-router";
import Welcome from "./Welcome/Welcome";
import SetupWallet from "./SetupWallet/SetupWallet";
import Improve from "./Improve/Improve";
import CreateWallet from "./CreateWallet/CreateWallet";
import ImportWallet from "./ImportWallet/ImportWallet";
import SeedPhrase from "./SeedPhrase/SeedPhrase";
import ConfirmSeedWords from "./ConfirmSeedWords/ConfirmSeedWords";
import Complete from "./Complete/Complete";

function Onboarding() {
  return (
    <div className="onboarding">
      <Routes>
        <Route path="welcome" element={<Welcome />} />
        <Route path="setup-wallet" element={<SetupWallet />} />
        <Route path="improve" element={<Improve />} />
        <Route path="create-wallet" element={<CreateWallet />} />
        <Route path="import-wallet" element={<ImportWallet />} />
        <Route path="seed-phrase" element={<SeedPhrase />} />
        <Route path="confirm-seed-words" element={<ConfirmSeedWords />} />
        <Route path="complete" element={<Complete />} />
        <Route path="" element={<Navigate replace to="welcome" />} />
      </Routes>
    </div>
  );
}

export default Onboarding;
