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
import React from "react";
import {
  Alert,
  Box,
  Button,
  Container,
  FormControl,
  FormControlLabel,
  FormGroup,
  Grid,
  List,
  ListItem,
  ListItemText,
  Stack,
  Switch,
  TextField,
  Typography,
} from "@mui/material";
import binding from "./binding";
import {withRouter} from "react-router-dom";

class Create extends React.Component {
  constructor(props) {
    super(props);

    this.state = {
      name: "Asset1",
      description: "",
      image: "https://source.unsplash.com/random",
      error: "",
      isSaving: false,
      contract001: true,
      contract721: false,
      contract100: false,
      numberInitialTokens: 0,
      committee: [],
      newCommitteePubKey: "",
    };

    this.save = this.save.bind(this);
    this.onNameChanged = this.onNameChanged.bind(this);
    this.onDescriptionChanged = this.onDescriptionChanged.bind(this);
    this.onImageChanged = this.onImageChanged.bind(this);
    this.onContract001Changed = this.onContract001Changed.bind(this);
    this.onContract721Changed = this.onContract721Changed.bind(this);
    this.onContract100Changed = this.onContract100Changed.bind(this);
    this.onNumberInitialTokensChanged =
      this.onNumberInitialTokensChanged.bind(this);
    this.onDeleteCommitteeMember = this.onDeleteCommitteeMember.bind(this);
    this.onNewCommitteePubKeyChanged =
      this.onNewCommitteePubKeyChanged.bind(this);
    this.onAddCommitteeMember = this.onAddCommitteeMember.bind(this);
  }

  async save() {
    this.setState({ isSaving: true });
    let name = this.state.name;
    let description = this.state.description;
    let image = this.state.image;
    try {
      let publicKey = await binding.command_assets_create(
        name,
        description,
        image
      );

      if (this.state.contract721 || this.state.contract100) {
        let res = await binding.command_asset_issue_simple_tokens(
            publicKey,
            parseInt(this.state.numberInitialTokens),
            this.state.committee
        );

        console.log(res);
      }
      let history = this.props.history;

      history.push(`/assets/manage/${publicKey}`);
    } catch (err) {
      this.setState({
        error: "Could not create asset: " + err,
      });
      console.log(err);
    }
    this.setState({ isSaving: false });
  }

  onNameChanged(e) {
    this.setState({ name: e.target.value });
  }

  onContract001Changed(e) {
    this.setState({ contract001: e.target.checked });
  }

  onContract721Changed(e) {
    this.setState({ contract721: e.target.checked });
  }

  onContract100Changed(e) {
    this.setState({ contract100: e.target.checked });
  }

  onNumberInitialTokensChanged(e) {
    this.setState({ numberInitialTokens: e.target.value });
  }

  onDescriptionChanged(e) {
    this.setState({
      description: e.target.value,
    });
  }

  onNewCommitteePubKeyChanged(e) {
    this.setState({
      newCommitteePubKey: e.target.value,
    });
  }

  onAddCommitteeMember() {
    let committee = [...this.state.committee];
    committee.push(this.state.newCommitteePubKey);
    console.log(committee);
    this.setState({
      committee,
      newCommitteePubKey: "",
    });
  }

  onDeleteCommitteeMember(index) {
    let committee = this.state.committee.filter(function (_, i, arr) {
      return i != index;
    });

    this.setState({ committee });
  }

  onImageChanged(e) {
    this.setState({
      image: e.target.value,
    });
  }

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
                onChange={this.onContract001Changed}
                checked={this.state.contract001}
              />
            }
            label="001 Metadata (required)"
          />

          <FormGroup>
            <TextField
              id="name"
              label="Name"
              variant="filled"
              color="primary"
              value={this.state.name}
              onChange={this.onNameChanged}
              disabled={this.state.isSaving || !this.state.contract001}
            ></TextField>
            <TextField
              id="description"
              label="Description"
              variant="filled"
              color="primary"
              value={this.state.description}
              onChange={this.onDescriptionChanged}
              disabled={this.state.isSaving || !this.state.contract001}
            ></TextField>
            <TextField
              id="image"
              label="Image (url)"
              variant="filled"
              color="primary"
              value={this.state.image}
              onChange={this.onImageChanged}
              disabled={this.state.isSaving || !this.state.contract001}
            ></TextField>

            <FormControlLabel
              control={
                <Switch
                  onClick={this.onContract721Changed}
                  checked={this.state.contract721}
                />
              }
              label="721 (ERC 721-like)"
            />
            <TextField
              id="numTokens"
              value={this.state.numberInitialTokens}
              onChange={this.onNumberInitialTokensChanged}
              type="number"
              label="Number of initial tokens"
              disabled={this.state.isSaving || !this.state.contract721}
            ></TextField>
          </FormGroup>
          <FormGroup>
            <FormControlLabel
              control={
                <Switch
                  onClick={this.onContract100Changed}
                  checked={this.state.contract100}
                />
              }
              label="100 Sidechain with committees"
            />
          </FormGroup>
          <FormGroup>
            <List>
              {this.state.committee.map((item, index) => {
                return (
                  <ListItem>
                    <ListItemText primary={item}></ListItemText>
                  </ListItem>
                );
              })}
            </List>
            <TextField
              label="New public key"
              id="newCommitteePubKey"
              value={this.state.newCommitteePubKey}
              onChange={this.onNewCommitteePubKeyChanged}
              disabled={this.state.isSaving || !this.state.contract100}
            ></TextField>
            <Button
              onClick={this.onAddCommitteeMember}
              disabled={this.state.isSaving || !this.state.contract100}
            >
              Add
            </Button>
          </FormGroup>

          <Button onClick={this.save} disabled={this.state.isSaving}>
            Save
          </Button>
        </Stack>
      </Container>
    );
  }
}

export default withRouter(Create)

