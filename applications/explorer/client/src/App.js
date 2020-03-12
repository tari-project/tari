import React, {useState, useEffect} from "react";
import {
    BrowserRouter as Router,
    Switch,
    Route,
    Link,
    useParams
} from "react-router-dom";
import axios from 'axios';

export default function App() {
    return (
        <Router>
            <div>
                <nav>
                    <ul>
                        <li>
                            <Link to="/">Home</Link>
                        </li>

                        <li>
                            <Link to="/orphans">Orphaned Blocks</Link>
                        </li>
                    </ul>
                </nav>

                {/* A <Switch> looks through its children <Route>s and
            renders the first one that matches the current URL. */}
                <Switch>
                    <Route path="/blocks/:hash">
                        <Block/>
                    </Route>
                    <Route path="/orphans">
                        <Orphans/>
                    </Route>
                    <Route path="/">
                        <Home/>
                    </Route>
                </Switch>
            </div>
        </Router>
    );
}

function Home() {

    const [state, setState] = useState('');
    useEffect(() => {
        axios.get('http://localhost:8081/blocks')
            .then(res => {
                let blocks = res.data;
                setState({blocks: blocks})
            });
    }, []);


    return (<div><h2>Home</h2>

            <table>
                <thead>
                <tr>
                    <th>Height</th>
                    <th>Hash</th>
                    <th>Timestamp</th>
                    <th>Proof of Work</th>
                    <th>Difficulty</th>
                    <th>Utxos</th>
                </tr>
                </thead>
                <tbody>
                {state.blocks ? state.blocks.map(block => {
                    const utcSeconds = block.timestamp;
                    const time = new Date(0); // The 0 there is the key, which sets the date to the epoch
                    time.setUTCSeconds(utcSeconds);

                    return (<tr>
                            <td>{block.height}</td>
                            <td><Link to={"/blocks/" + block.hash}>{block.hash}</Link></td>
                            <td>{time.toISOString()}</td>
                            <td>{block.proof_of_work.pow_algo}</td>
                            <td>{block.proof_of_work.accumulated_blake_difficulty}</td>
                            <td>{block.utxos}</td>
                        </tr>
                    );
                }) : ""}</tbody>
            </table>
        </div>
    )
}

function Block() {
    const {hash} = useParams();


    const [state, setState] = useState('');


    useEffect(() => {
        axios.get('http://localhost:8081/blocks?hash=' + hash)
            .then(res => {
                let block = res.data;
                setState({block: block})
            });
    }, [hash]);

    return (<div><h2>Block: {state.block ? state.block.hash : "...loading"}</h2>

            <h3>Utxos Added:</h3>
            <table>

             <thead>
             <tr>
                 <th>Hash</th>
             </tr>
             </thead>
                <tbody>
                {
                    state.block && state.block.nodes_added ?
                        state.block.nodes_added.map( node => (
                            <tr><td>{node}</td></tr>
                        ))

                        : ""
                }

                </tbody>
            </table>
        </div>
        );
}

function Orphans() {
    const [state, setState] = useState('');
    useEffect(() => {
        axios.get('http://localhost:8081/orphans')
            .then(res => {
                let blocks = res.data;
                setState({blocks: blocks})
            });
    }, []);


    return (<div><h2>Orphan Blocks</h2>

            <table>
                <thead>
                <tr>
                    <th>Height</th>
                    <th>Hash</th>
                    <th>Timestamp</th>
                    <th>Proof of Work</th>
                    <th>Difficulty</th>
                </tr>
                </thead>
                <tbody>
                {state.blocks ? state.blocks.map(block => {
                    const utcSeconds = block.header.timestamp;
                    const time = new Date(0); // The 0 there is the key, which sets the date to the epoch
                    time.setUTCSeconds(utcSeconds);

                    return (<tr>
                            <td>{block.header.height}</td>
                            <td><Link to={"/blocks/" + block.hash}>{block.hash}</Link></td>
                            <td>{time.toISOString()}</td>
                            <td>{block.header.proof_of_work.pow_algo}</td>
                            <td>{block.header.proof_of_work.accumulated_blake_difficulty}</td>
                        </tr>
                    );
                }) : ""}</tbody>
            </table>
        </div>
    )
}