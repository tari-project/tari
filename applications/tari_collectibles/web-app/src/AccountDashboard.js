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
import {Alert, Button, Container, FormGroup, Grid, Paper, Stack, TextField, Typography} from "@mui/material";
import binding from "./binding";
import protobuf from "protobufjs";

class AccountDashboard extends React.Component {
    constructor(props) {
        super(props);

        console.log(props);
        this.state = {
            error: null,
            isSaving: false,
            tip002: false,
            tip004: false,
            tip721: false,
            tip002Data: {},
            tip721Data: {},
            assetPubKey: props.match.params.assetPubKey,
            balance: -1,
            receiveAddress: "",
            sendToAddress: "",
            tip721SendDraftId: ""
        };
    }

    async componentDidMount() {
        try {

            let receiveAddress = await binding.command_asset_wallets_get_latest_address(this.state.assetPubKey);
            let assetInfo = await binding.command_assets_get_registration(this.state.assetPubKey);
            console.log(assetInfo);
            let tip002 = assetInfo.features["template_ids_implemented"].includes(2);
            let tip004 = assetInfo.features["template_ids_implemented"].includes(4);
            let tip721 = assetInfo.features["template_ids_implemented"].includes(721);
            let tip002Data = {};
            if (tip002) {
                await this.refreshBalance();
                let templateParams = assetInfo.features["template_parameters"];
                let tip002Params = templateParams.filter((item) => {
                    return item.template_id === 2
                })[0];

                await protobuf.load("/proto/tip002.proto").then(function (root) {
                    let InitRequest = root.lookupType("tip002.InitRequest");
                    let message = InitRequest.decode(tip002Params["template_data"]);
                    tip002Data = InitRequest.toObject(message, {});
                    console.log(tip002Data);
                });
            }
            let tip721Data = {};
            if (tip004) {
               tip721Data = await this.refresh721();
            }
            this.setState({receiveAddress: receiveAddress.public_key, tip002, tip004, tip721, tip002Data, tip721Data});
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

    refresh721 = async() => {
        let tip721Data = {};
            let tokens = await binding.command_tip004_list_tokens(this.state.assetPubKey);
            console.log(tokens);
            tip721Data.tokens = [];
            await tokens.forEach((token) => {
                tip721Data.tokens.push({
                    tokenId: token[0].token_id,
                    address: token[1].public_key,
                    addressId: token[1].id,
                    token: token[0].token
                })
            });
            return tip721Data;
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
    openTip721SendDraft = async(tokenId) => {
       this.setState({
           tip721SendDraftId: tokenId
       })
    }
    on721Send = async(fromAddressId, tokenId) => {
        try {
            this.setState({error: ""});
            let result = await binding.command_tip721_transfer_from(this.state.assetPubKey, fromAddressId, this.state.sendToAddress,  tokenId);
            console.log(result);
            let tip721Data = await this.refresh721();
            this.setState({ tip721Data});
            await this.refreshBalance();
        } catch (err) {
            console.error("Error sending:", err);
            this.setState({error: err.message});
        }
    }

    render() {
        return (
            <Container sx={{mt: 4, mb: 4}}>
                <Grid container spacing={2}>
                    <Grid item xs={12}>
                        {this.state.error ? (
                            <Alert severity="error">{this.state.error}</Alert>
                        ) : (
                            <span/>
                        )}
                        <Typography variant="h3" sx={{mb: "30px"}}>
                            Asset Details
                        </Typography>
                        <Container>
                            <Typography variant="h4">Info</Typography>
                            <Stack spacing="2">
                                <Typography>
                                    Pub key: {this.state.assetPubKey}
                                </Typography>
                                <Typography>Receive Address: {this.state.receiveAddress}</Typography>
                                <Button onClick={this.onGenerateReceiveAddress}>Generate new receive
                                    address</Button>
                            </Stack>
                        </Container>
                    </Grid>
                    <Grid item xs={3} hidden={!this.state.tip002}>
                        <Paper>
                            <Container sx={{pt: 2}}>
                                <Stack spacing={2}>
                                    <Typography variant="h5">TIP002</Typography>
                                    <Typography>Balance: {this.state.balance / Math.pow(10, this.state.tip002Data.decimals)} {this.state.tip002Data.symbol}</Typography>

                                    <h6>Send</h6>
                                    <TextField onChange={this.onSendToChanged}
                                               value={this.state.sendToAddress} label="Receiver address"></TextField>
                                    <TextField onChange={this.onSendToAmountChanged} value={this.state.sendToAmount}
                                               type="number" label="Amount"></TextField>
                                    <Button onClick={this.onSend}>Send</Button>
                                </Stack>
                            </Container>
                        </Paper>
                    </Grid>
                    {this.state.tip721 ? (<Grid item xs={12} hidden={!this.state.tip721}>
                                <Stack spacing={2}>
                                    <Typography variant="h5">TIP721</Typography>
                                    <Grid container spacing={2}>
                                        {this.state.tip721Data.tokens.map((token) => {
                                            return (<Grid item xs={2}>
                                                <Paper>
                                                    <Container>
                                                        <Stack spacing={2}><Typography variant="h6">#{token.tokenId}: {token.token}</Typography>
                                                        <Button onClick={(e) => this.openTip721SendDraft(token.tokenId)}>Send</Button>
                                                            { this.state.tip721SendDraftId === token.tokenId ? (
                                                                <Paper elevation={2}>
                                                                    <Container>
                                                                        <Stack spacing={2}>
                                                                            <TextField value={this.state.sendToAddress} onChange={this.onSendToChanged} label="To"></TextField>
                                                                            <Button onClick={(e) => this.on721Send(token.addressId, token.tokenId)}>Submit</Button>
                                                                        </Stack>
                                                                    </Container>
                                                                </Paper>
                                                            ) : ""}
                                                        </Stack>
                                                    </Container>
                                                </Paper>

                                            </Grid>)
                                        })}
                                    </Grid>
                                </Stack>
                    </Grid>) : ""}

                </Grid>
            </Container>

        );
    }
}

export default withRouter(AccountDashboard);
