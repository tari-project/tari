//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

import React, { useState, useMemo } from "react";
import {
  Alert,
  Button,
  Container,
  FormControlLabel,
  FormGroup,
  List,
  ListItem,
  ListItemIcon,
  ListItemText,
  Stack,
  Switch,
  TextField,
  Typography,
} from "@mui/material";
import { DeleteForever } from "@mui/icons-material";
import binding from "./binding";
import { withRouter } from "react-router-dom";
import { appWindow } from "@tauri-apps/api/window";
import { dialog } from "@tauri-apps/api";
import { Command } from "@tauri-apps/api/shell";
import { fetch, ResponseType } from "@tauri-apps/api/http";
import protobuf from "protobufjs";
import PropTypes from "prop-types";

class Create extends React.Component {
  constructor(props) {
    super(props);

    this.state = {
      name: "Asset1",
      description: "",
      image: "",
      cid: "",
      error: "",
      imageError: null,
      isSaving: false,
      tip001: true,
      tip002: false,
      tip003: true,
      tip004: false,
      tip721: false,
      tip002Data: {
        totalSupply: 0,
        symbol: "NAH",
        decimals: 2,
      },
      tip003Data: {
        committee: [],
      },
      newCommitteePubKey: "",
      committeeEditorError: null,
      isValid: false,
      saveErrors: [],
    };

    this.cleanup = null;
  }

  componentDidMount() {
    this.cleanup = appWindow.listen("tauri://file-drop", (obj) =>
      this.dropFile(obj)
    );
    console.log("didmount");
  }

  async dropFile({ payload }) {
    if (payload.length > 0) {
      try {
        // only use the first file if multiple are dropped
        let cid = await this.addFileToIPFS(payload[0]);
        this.setState({ cid, imageError: null });
      } catch (e) {
        this.setState({ imageError: e.toString() });
      }
    }
  }

  componentWillUnmount() {
    if (this.cleanup) {
      this.cleanup.then((unlisten) => unlisten());
    }
  }

  save = async () => {
    const isValid = await this.validate();
    if (!isValid) {
      return;
    }
    this.setState({ isSaving: true });
    let { name, description, image } = this.state;
    try {
      let templateIds = [1];
      let templateParameters = [];
      if (this.state.tip002) {
        console.log("tip002");
        templateIds.push(2);
        console.log(this.state.tip002Data);
        let payload = {
          symbol: this.state.tip002Data.symbol,
          decimals: parseInt(this.state.tip002Data.decimals),
          totalSupply: parseInt(this.state.tip002Data.totalSupply),
        };

        await protobuf.load("proto/tip002.proto").then(function (root) {
          let InitRequest = root.lookupType("tip002.InitRequest");

          var errMsg = InitRequest.verify(payload);
          if (errMsg) {
            throw new Error(errMsg);
          }
          var message = InitRequest.create(payload);
          console.log("msg:", message);
          var buffer = InitRequest.encode(message);
          let arr = buffer.finish();
          console.log("buffer", arr);

          templateParameters.push({
            template_id: 2,
            template_data_version: 1,
            template_data: Array.from(arr),
          });
        });
      }

      if (this.state.tip003) {
        templateIds.push(3);
      }
      if (this.state.tip004) {
        templateIds.push(4);
      }
      if (this.state.tip721) {
        templateIds.push(721);
      }

      let outputs = await binding.command_asset_wallets_get_unspent_amounts();

      if (outputs.length <= 1) {
        throw { message: "You need at least two unspent outputs" };
      }
      let publicKey = await binding.command_assets_create(
        name,
        description,
        image,
        templateIds,
        templateParameters
      );

      // TODO: How to create the initial checkpoint?
      if (this.state.tip003) {
        let asset_registration =
          await binding.command_asset_create_initial_checkpoint(publicKey);
        console.log(asset_registration);
        let committee_definition =
          await binding.command_asset_create_committee_definition(
            publicKey,
            this.state.tip003Data.committee
          );
        console.log(committee_definition);
      }
      let history = this.props.history;

      history.push(`/assets/manage/${publicKey}`);
    } catch (err) {
      console.error(err);
      this.setState({
        error: "Could not create asset: " + err.message,
      });
    }
    this.setState({ isSaving: false });
  };

  onNameChanged = (e) => {
    this.setState({ name: e.target.value });
  };

  onTipCheckboxChanged = (e, tip) => {
    let obj = {};
    obj[tip] = e.target.checked;
    this.setState(obj);
  };

  onTip002DataChanged = (field, e) => {
    let tip002Data = {};
    tip002Data[field] = e.target.value;
    tip002Data = { ...this.state.tip002Data, ...tip002Data };
    this.setState({ tip002Data: tip002Data });
  };

  // onNumberInitialTokensChanged = (e) => {
  //   this.setState({ numberInitialTokens: e.target.value });
  // };

  onDescriptionChanged = (e) => {
    this.setState({
      description: e.target.value,
    });
  };

  onNewCommitteePubKeyChanged = (e) => {
    this.setState({
      newCommitteePubKey: e.target.value,
    });
  };

  onAddCommitteeMember = () => {
    let pubKey = this.state.newCommitteePubKey;
    if (!pubKey) return;
    pubKey = pubKey.trim();
    if (!pubKey) return;
    if (this.state.tip003Data.committee.includes(pubKey)) {
      this.setState({ committeeEditorError: "Public key already added!" });
      return;
    }

    let committee = [...this.state.tip003Data.committee];
    committee.push(pubKey);
    let tip003Data = {
      ...this.state.tip003Data,
      ...{ committee: committee },
    };
    console.log(committee);

    this.setState({
      tip003Data,
      saveErrors: [],
      newCommitteePubKey: "",
      committeeEditorError: null,
    });
  };

  onDeleteCommitteeMember = (index) => {
    let committee = this.state.tip003Data.committee.filter(function (
      _,
      i,
      arr
    ) {
      return i !== parseInt(index);
    });
    let tip003Data = { ...this.state.tip003Data, ...{ committee } };

    this.setState({ tip003Data });
  };

  async validate() {
    const { tip003, tip003Data } = this.state;
    const saveErrors = [];

    if (tip003 && tip003Data.committee.length === 0) {
      saveErrors.push("Must add at least one committee member");
    }
    await this.setState({
      saveErrors,
    });
    return saveErrors.length === 0;
  }

  onImageChanged = (e) => {
    this.setState({
      image: e.target.value,
    });
  };

  selectFile = async () => {
    const filePath = await dialog.open({
      filters: [
        {
          name: "image types",
          extensions: ["png", "jpg", "jpeg", "gif"],
        }, // TODO more formats
      ],
      multiple: false,
    });
    try {
      let cid = await this.addFileToIPFS(filePath);
      console.info("IPFS cid: ", cid);
      this.setState({ cid, imageError: null });
    } catch (e) {
      this.setState({ imageError: e.toString() });
    }
  };

  addFileToIPFS = async (filePath) => {
    const parts = filePath.split("/");
    const name = parts[parts.length - 1];
    const timestamp = new Date().valueOf();
    // unfortunately the ipfs http /add api doesn't play nicely with the tauri http client
    // resulting in "file argument 'path' is required"
    // so we'll shell out to the ipfs command

    const command = new Command("ipfs", ["add", "-Q", filePath]);
    await command.spawn();

    let cid = await new Promise((resolve, reject) => {
      let processing = false;
      let cid;
      command.on("close", (data) => {
        if (data.code === 0) {
          resolve(cid);
        }
      });
      command.stdout.on("data", (line) => {
        if (!processing) {
          processing = true;
          cid = line;
        }
      });
      command.on("error", (line) => {
        reject(new Error(line));
      });
    });

    if (!cid) return;

    const cp = new Command("ipfs", [
      "files",
      "cp",
      `/ipfs/${cid}`,
      `/${timestamp}${name}`,
    ]);
    await cp.spawn();

    await new Promise((resolve, reject) => {
      cp.on("close", (data) => {
        if (data.code === 0) {
          resolve(null);
        } else {
          reject("IPFS command exited with code ", data.code);
        }
      });

      cp.stderr.on("data", (line) => {
        reject(new Error(`${line}. Ensure that IPFS is running.`));
      });
    });

    return cid;
  };

  render() {
    return (
      <Container maxWidth="lg" sx={{ mt: 4, mb: 4, py: 8 }}>
        <Typography variant="h3" sx={{ mb: "30px" }}>
          Create new asset
        </Typography>
        <Stack>
          {this.state.error ? (
            <Alert severity="error">{this.state.error}</Alert>
          ) : (
            <span />
          )}
          <Typography variant="h4">Templates Implemented</Typography>
          <FormControlLabel
            control={
              <Switch
                onChange={this.onTip001Changed}
                checked={this.state.tip001}
              />
            }
            label="001 Metadata (required)"
          />

          <FormGroup>
            <TextField
              id="publicKey"
              label="Public Key"
              variant="filled"
              color="secondary"
              value={this.state.publicKey}
              disabled
              style={{ "-webkit-text-fill-color": "#ddd" }}
            />
            <TextField
              id="name"
              label="Name"
              variant="filled"
              color="primary"
              value={this.state.name}
              onChange={this.onNameChanged}
              disabled={this.state.isSaving || !this.state.tip001}
            />
            <TextField
              id="description"
              label="Description"
              variant="filled"
              color="primary"
              value={this.state.description}
              onChange={this.onDescriptionChanged}
              disabled={this.state.isSaving || !this.state.tip001}
            />

            <p>Image</p>
            <ImageSelector
              selectFile={this.selectFile}
              setImage={(image) => this.setState({ image })}
              cid={this.state.cid}
              setCid={(cid) => this.setState({ cid })}
              image={this.state.image}
              error={this.state.imageError}
              setError={(e) => this.setState({ imageError: e })}
            />
          </FormGroup>
          <FormGroup>
            <FormControlLabel
              control={
                <Switch
                  onClick={(e) => this.onTipCheckboxChanged(e, "tip002")}
                  checked={this.state.tip002}
                />
              }
              label="002 (ERC 20-like)"
            />

            <TextField
              id="tip002_symbol"
              label="Symbol"
              variant="filled"
              color="primary"
              value={this.state.tip002.symbol}
              onChange={(e) => this.onTip002DataChanged("symbol", e)}
              disabled={this.state.isSaving || !this.state.tip002}
            />

            <TextField
              id="tip002_total_supply"
              label="Total Supply"
              variant="filled"
              color="primary"
              value={this.state.tip002.totalSupply}
              type="number"
              onChange={(e) => this.onTip002DataChanged("totalSupply", e)}
              disabled={this.state.isSaving || !this.state.tip002}
            />

            <TextField
              id="tip002_decimals"
              label="Decimals"
              variant="filled"
              color="primary"
              value={this.state.tip002.decimals}
              onChange={(e) => this.onTip002DataChanged("decimals", e)}
              disabled={this.state.isSaving || !this.state.tip002}
            />
          </FormGroup>
          <FormGroup>
            <FormControlLabel
              control={
                <Switch
                  onClick={this.onTip003Changed}
                  checked={this.state.tip003}
                  disabled={true}
                />
              }
              label="003 Sidechain with committees"
            />
          </FormGroup>
          <CommitteeEditor
            members={this.state.tip003Data.committee}
            onNewCommitteePubKeyChanged={this.onNewCommitteePubKeyChanged}
            newCommitteePubKey={this.state.newCommitteePubKey}
            onAddCommitteeMember={this.onAddCommitteeMember}
            onDeleteCommitteeMember={this.onDeleteCommitteeMember}
            disabled={this.state.isSaving || !this.state.tip003}
            error={this.state.committeeEditorError}
          />
          <FormGroup>
            <FormControlLabel
              control={
                <Switch
                  onClick={(e) => this.onTipCheckboxChanged(e, "tip721")}
                  checked={this.state.tip721}
                />
              }
              label="721 (ERC 721-like)"
            />
          </FormGroup>
          <FormGroup>
            <FormControlLabel
              control={
                <Switch
                  onClick={(e) => this.onTipCheckboxChanged(e, "tip004")}
                  checked={this.state.tip004}
                />
              }
              label="Allow tokens to be minted after registration (Requires 721)"
            />
          </FormGroup>
          <Button onClick={this.save} disabled={this.state.isSaving}>
            Save
          </Button>

          {this.state.saveErrors.length > 0 ? (
            <div>
              {this.state.saveErrors.map((e) => (
                <Alert key={e.toString()} severity="error">
                  {e.toString()}
                </Alert>
              ))}
            </div>
          ) : (
            <span />
          )}
        </Stack>
      </Container>
    );
  }
}

Create.propTypes = {
  history: PropTypes.object,
};

const ImageSwitch = ({ setMode }) => {
  return (
    <div>
      <Button onClick={() => setMode("url")}>HTTP or IPFS URL</Button>
      <Button onClick={() => setMode("upload")}>Upload file to IPFS</Button>
    </div>
  );
};

ImageSwitch.propTypes = {
  setMode: PropTypes.func,
};

const ImageUrl = ({ setImage, setMode }) => {
  const [url, setUrl] = useState("");

  return (
    <div>
      <p>Link to a web or ipfs url</p>
      <TextField
        id="image"
        label="Image (url)"
        variant="filled"
        color="primary"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
      />
      <Button onClick={() => setImage(url)}>Save</Button>
      <Button onClick={() => setMode("")}>Back</Button>
    </div>
  );
};

ImageUrl.propTypes = {
  setImage: PropTypes.func,
  setMode: PropTypes.func,
};

const ImageUpload = ({ selectFile, setMode }) => {
  return (
    <div>
      <p>Select an image, or drag and drop an image onto this window</p>
      <Button onClick={selectFile}>Click to Select Image</Button>
      <Button onClick={() => setMode("")}>Back</Button>
    </div>
  );
};

ImageUpload.propTypes = {
  selectFile: PropTypes.func,
  setMode: PropTypes.func,
  error: PropTypes.string,
};

const ImageSelector = ({
  cid,
  image,
  selectFile,
  setImage,
  setCid,
  error,
  setError,
}) => {
  const [mode, setMode] = useState("");
  const reset = () => {
    setError("");
    setMode("");
    setImage("");
  };

  if (error) {
    return (
      <div>
        <Alert severity="error">{error}</Alert>
        <Button onClick={reset}>Reset</Button>
      </div>
    );
  }

  if (image) {
    return (
      <div>
        <img src={image} alt="" width="200px" />
        <br />
        <Button onClick={reset}>Reset</Button>
      </div>
    );
  }
  if (cid) {
    return <IpfsImage cid={cid} setCid={setCid} reset={reset} />;
  }

  let display;

  switch (mode) {
    case "url":
      display = <ImageUrl setImage={setImage} setMode={setMode} />;
      break;
    case "upload":
      display = <ImageUpload selectFile={selectFile} setMode={setMode} />;
      break;
    default:
      display = <ImageSwitch setMode={setMode} />;
  }

  return display;
};

ImageSelector.propTypes = {
  cid: PropTypes.string,
  image: PropTypes.string,
  selectFile: PropTypes.func,
  setImage: PropTypes.func,
  setCid: PropTypes.func,
  error: PropTypes.string,
  setError: PropTypes.func,
};

const IpfsImage = ({ cid, setCid, reset, error }) => {
  const [src, setSrc] = useState("");
  const [httpError, setHttpError] = useState(null);

  useMemo(async () => {
    try {
      const response = await fetch(
        `http://localhost:5001/api/v0/cat?arg=${cid}`,
        {
          method: "POST",
          responseType: ResponseType.Binary,
        }
      );
      const typedArray = Uint8Array.from(response.data);
      const blob = new Blob([typedArray.buffer]);
      const reader = new FileReader();
      reader.onloadend = () => setSrc(reader.result);
      reader.onerror = () => console.error("error");
      reader.readAsDataURL(blob);
    } catch (e) {
      setHttpError(e);
    }
  }, [cid]);

  if (src) {
    return (
      <div>
        <img src={src} alt="" width="200px" />
        <br />
        <Button
          onClick={() => {
            reset();
            setSrc("");
            setCid("");
          }}
        >
          Reset
        </Button>
      </div>
    );
  }

  if (error || httpError) {
    return (
      <div>
        <Alert severity="error">
          {error || httpError} - is the IPFS daemon running?
        </Alert>
        <Button
          onClick={() => {
            setCid("");
            reset();
          }}
        >
          Reset
        </Button>
      </div>
    );
  }

  return <p>ipfs image loading...</p>;
};

IpfsImage.propTypes = {
  cid: PropTypes.string,
  setCid: PropTypes.func,
  reset: PropTypes.func,
  error: PropTypes.string,
};

const CommitteeEditor = ({
  members,
  disabled,
  onAddCommitteeMember,
  onDeleteCommitteeMember,
  onNewCommitteePubKeyChanged,
  newCommitteePubKey,
  error,
}) => {
  return (
    <FormGroup>
      <List>
        {members.map((item, index) => {
          return (
            <ListItem
              key={item}
              button
              component="a"
              onClick={() =>
                onDeleteCommitteeMember && onDeleteCommitteeMember(index)
              }
              disabled={disabled}
            >
              <ListItemText primary={item} />
              <ListItemIcon>
                <DeleteForever />
              </ListItemIcon>
            </ListItem>
          );
        })}
      </List>
      <TextField
        label="Validator node public key"
        id="newCommitteePubKey"
        value={newCommitteePubKey}
        onChange={onNewCommitteePubKeyChanged}
        disabled={disabled}
      />
      <Button onClick={onAddCommitteeMember} disabled={disabled}>
        Add
      </Button>
      {error ? <Alert severity="warning">{error}</Alert> : <span />}
    </FormGroup>
  );
};

CommitteeEditor.propTypes = {
  members: PropTypes.array.isRequired,
  disabled: PropTypes.bool,
  onAddCommitteeMember: PropTypes.func,
  onDeleteCommitteeMember: PropTypes.func,
  onNewCommitteePubKeyChanged: PropTypes.func,
  newCommitteePubKey: PropTypes.string,
  error: PropTypes.string,
};

export default withRouter(Create);
