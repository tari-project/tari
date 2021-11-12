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
import {withRouter} from "react-router-dom";
import {Alert, Button, Container, Stack, TextField, Typography} from "@mui/material";
import binding from "./binding";

class NewAccount extends React.Component {
    constructor(props) {
        super(props);

        this.state = {
            error: null,
            isSaving: false,
            assetPublicKey: ""
        };
    }

    onAssetPublicKeyChanged = (e) => {
        this.setState({ assetPublicKey: e.target.value });
    };

    onSave = async (e) => {
        e.preventDefault();
        console.log(this.state.assetPublicKey);
        this.setState({isSaving: true, error: null});
        try{
            await binding.command_accounts_create(this.state.assetPublicKey);
            let history = this.props.history;
let path =`/assets/watched/details/${this.state.assetPublicKey}`;
console.log(path);
            history.push(path);
            return;
        }catch(e) {
            this.setState({error: e});
            console.error(e);
        }
        this.setState({isSaving: false});
    }

    render() {
       return   (<Container maxWidth="lg" sx={{ mt: 4, mb: 4, py: 8 }}>
           <Typography variant="h3" sx={{ mb: "30px" }}>
               New asset account
           </Typography>
           <Stack>
               {this.state.error ? (
                   <Alert severity="error">{this.state.error}</Alert>
               ) : (
                   <span />
               )}
               <TextField
                   id="publicKey"
                   label="Asset Public Key"
                   variant="filled"
                   color="primary"
                   value={this.state.assetPublicKey}
                   onChange={this.onAssetPublicKeyChanged}
                   disabled={this.state.isSaving}
               ></TextField>
               <Button onClick={this.onSave} disabled={this.state.isSaving}>
                   Save
               </Button>
           </Stack>
       </Container>);
    }
}

export default withRouter(NewAccount);
