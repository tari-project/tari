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
import {withRouter} from "react-router-dom";
import {Alert, Button, Container, Stack, TextField, Typography} from "@mui/material";
import binding from "./binding";

class AccountDashboard extends React.Component {
    constructor(props) {
        super(props);

        console.log(props);
        this.state = {
            error: null,
            isSaving: false,
            tip101: false,
            tip102: false,
            assetPubKey: props.match.params.assetPubKey,
            balance: -1,
            receiveAddress: "",
            sendToAddress: ""
        };
    }

    async componentDidMount() {
        try {
            await this.refreshBalance();

            let receiveAddress = await binding.command_asset_wallets_get_latest_address(this.state.assetPubKey);
            this.setState({receiveAddress: receiveAddress.public_key});
        } catch (err) {
            console.error(err);
            this.setState({error: err.message});
        }
    }

    refreshBalance = async () => {
        this.setState({error: null});
        let balance = await binding.command_asset_wallets_get_balance(this.state.assetPubKey);
        console.log("balance", balance);
        this.setState({balance});
        return balance;
    }

    onGenerateReceiveAddress = async () => {
        console.log("hello");
        try {
            this.setState({error: null});
            let receiveAddress = await binding.command_asset_wallets_create_address(this.state.assetPubKey);
            console.log("new address", receiveAddress);
            this.setState({receiveAddress: receiveAddress.public_key});
        } catch (err) {
            console.error(err);
            this.setState({error: err.message});
        }
    }

    onSendToChanged = async (e) => {
        this.setState({sendToAddress: e.target.value});
    }

    onSendToAmountChanged = async (e) => {
        this.setState({sendToAmount: parseInt(e.target.value)});
    }
    onSend = async () => {
        try {
            this.setState({error: ""});
            let result = await binding.command_asset_wallets_send_to(this.state.assetPubKey, this.state.sendToAmount, this.state.sendToAddress);
            console.log(result);
            this.setState({
                sendToAddress: "", sendToAmount: ""
            });
            await this.refreshBalance();
        } catch (err) {
            console.error("Error sending:", err);
            this.setState({error: err.message});
        }
    }

    render() {
        return (<Container maxWidth="lg" sx={{mt: 4, mb: 4, py: 8}}>
                {this.state.error ? (
                    <Alert severity="error">{this.state.error}</Alert>
                ) : (
                    <span/>
                )}
                <Typography variant="h3" sx={{mb: "30px"}}>
                    Asset Details
                </Typography>
                <Typography>
                    {this.state.assetPubKey}
                </Typography>
                <Stack>
                    <Typography>Balance: {this.state.balance}</Typography>
                    <Typography>Receive Address: {this.state.receiveAddress}</Typography>
                    <Button onClick={this.onGenerateReceiveAddress}>Generate new receive address</Button>
                    <TextField onChange={this.onSendToChanged} value={this.state.sendToAddress}></TextField>
                    <TextField onChange={this.onSendToAmountChanged} value={this.state.sendToAmount}
                               type="number"></TextField>
                    <Button onClick={this.onSend}>Send</Button>
                </Stack>
            </Container>
        );
    }
}

export default withRouter(AccountDashboard);
