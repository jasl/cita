// CITA
// Copyright 2016-2017 Cryptape Technologies LLC.

// This program is free software: you can redistribute it
// and/or modify it under the terms of the GNU General Public
// License as published by the Free Software Foundation,
// either version 3 of the License, or (at your option) any
// later version.

// This program is distributed in the hope that it will be
// useful, but WITHOUT ANY WARRANTY; without even the implied
// warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
// PURPOSE. See the GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use libproto::blockchain::Proof;
use libproto::router::{MsgType, RoutingKey, SubModules};
use libproto::snapshot::{Cmd, Resp, SnapshotReq};
use libproto::Message;
use std::cell::Cell;
use std::convert::{TryFrom, TryInto};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use util::RwLock;

enum AckType {
    ChainAck,
    ExecutorAck,
    AuthAck,
    ConsensusAck,
    NetAck,
}

impl From<SubModules> for AckType {
    fn from(sub_modules: SubModules) -> Self {
        match sub_modules {
            SubModules::Chain => AckType::ChainAck,
            SubModules::Executor => AckType::ExecutorAck,
            SubModules::Auth => AckType::AuthAck,
            SubModules::Consensus => AckType::ConsensusAck,
            SubModules::Net => AckType::NetAck,
            _ => {
                error!("Wrong submodule: {:?}.", sub_modules);
                AckType::ChainAck
            }
        }
    }
}

#[derive(Clone)]
struct GotAck {
    // (bool, bool) = (whether or not received response, whether or not succeed)
    chain: Cell<(bool, bool)>,
    executor: Cell<(bool, bool)>,
    auth: Cell<(bool, bool)>,
    consensus: Cell<(bool, bool)>,
    net: Cell<(bool, bool)>,
}

impl Default for GotAck {
    fn default() -> Self {
        GotAck {
            chain: Cell::new((false, false)),
            executor: Cell::new((false, false)),
            auth: Cell::new((false, false)),
            consensus: Cell::new((false, false)),
            net: Cell::new((false, false)),
        }
    }
}

impl GotAck {
    // set ack with received msgs.
    pub fn set(&self, ack: AckType, is_succeed: bool) {
        match ack {
            AckType::ChainAck => self.chain.set((true, is_succeed)),
            AckType::ExecutorAck => self.executor.set((true, is_succeed)),
            AckType::AuthAck => self.auth.set((true, is_succeed)),
            AckType::ConsensusAck => self.consensus.set((true, is_succeed)),
            AckType::NetAck => self.net.set((true, is_succeed)),
        }
    }

    // reset ack
    pub fn reset(&self, ack: AckType) {
        match ack {
            AckType::ChainAck => self.chain.set((false, false)),
            AckType::ExecutorAck => self.executor.set((false, false)),
            AckType::AuthAck => self.auth.set((false, false)),
            AckType::ConsensusAck => self.consensus.set((false, false)),
            AckType::NetAck => self.net.set((false, false)),
        }
    }

    // whether or not received response.
    pub fn get(&self, ack: AckType) -> bool {
        match ack {
            AckType::ChainAck => self.chain.get().0,
            AckType::ExecutorAck => self.executor.get().0,
            AckType::AuthAck => self.auth.get().0,
            AckType::ConsensusAck => self.consensus.get().0,
            AckType::NetAck => self.net.get().0,
        }
    }

    // whether or not received response and the result is succeed.
    pub fn is_succeed(&self, ack: AckType) -> bool {
        match ack {
            AckType::ChainAck => self.chain.get().0 && self.chain.get().1,
            AckType::ExecutorAck => self.executor.get().0 && self.executor.get().1,
            AckType::AuthAck => self.auth.get().0 && self.auth.get().1,
            AckType::ConsensusAck => self.consensus.get().0 && self.consensus.get().1,
            AckType::NetAck => self.net.get().0 && self.net.get().0,
        }
    }
}

pub struct SnapShot {
    ctx_pub: Sender<(String, Vec<u8>)>,
    start_height: u64,
    end_height: u64,
    file: String,
    acks: GotAck,
    proof: RwLock<Proof>,
    restore_height: Cell<u64>,
}

impl SnapShot {
    pub fn new(
        ctx_pub: Sender<(String, Vec<u8>)>,
        start_height: u64,
        end_height: u64,
        file: String,
    ) -> Self {
        SnapShot {
            ctx_pub: ctx_pub,
            start_height: start_height,
            end_height: end_height,
            file: file,
            acks: GotAck::default(),
            proof: RwLock::new(Proof::new()),
            restore_height: Cell::new(0),
        }
    }

    // parse resp data
    pub fn parse_data(&self, key: String, msg_vec: Vec<u8>) -> bool {
        let mut msg = Message::try_from(&msg_vec).unwrap();

        let routing_key = RoutingKey::from(&key);

        if routing_key.is_msg_type(MsgType::SnapshotResp) {
            self.parse_resp(&mut msg, routing_key)
        } else {
            error!("error MsgClass!!!!");
            false
        }
    }

    fn parse_resp(&self, msg: &mut Message, routing_key: RoutingKey) -> bool {
        let sub_module = routing_key.get_sub_module();

        let snapshot_resp = msg.take_snapshot_resp().unwrap();

        self.acks.set(sub_module.clone().into(), snapshot_resp.flag);
        info!("snapshot_resp = {:?}", snapshot_resp);

        match snapshot_resp.resp {
            Resp::SnapshotAck => {
                info!("receive snapshot ack");
                self.acks.get(AckType::ChainAck) && self.acks.get(AckType::ExecutorAck)
            }
            Resp::BeginAck => {
                info!("receive begin ack");
                if self.acks.get(AckType::AuthAck)
                    && self.acks.get(AckType::ConsensusAck)
                    && self.acks.get(AckType::NetAck)
                {
                    self.acks.reset(AckType::AuthAck);
                    self.acks.reset(AckType::ConsensusAck);
                    self.acks.reset(AckType::NetAck);
                    self.restore();
                }

                false
            }
            Resp::RestoreAck => {
                info!("receive restore ack, sub_module = {:?}", sub_module);
                if sub_module == SubModules::Chain {
                    *self.proof.write() = snapshot_resp.get_proof().clone();
                    self.restore_height.set(snapshot_resp.get_height());
                }

                if self.acks.get(AckType::ChainAck) && self.acks.get(AckType::ExecutorAck) {
                    if !self.acks.is_succeed(AckType::ChainAck)
                        || !self.acks.is_succeed(AckType::ExecutorAck)
                    {
                        self.end();
                    } else {
                        self.clear();
                    }

                    self.acks.reset(AckType::ChainAck);
                    self.acks.reset(AckType::ExecutorAck);
                }

                false
            }
            Resp::ClearAck => {
                info!("receive clear ack");
                if self.acks.get(AckType::AuthAck)
                    && self.acks.get(AckType::ConsensusAck)
                    && self.acks.get(AckType::NetAck)
                {
                    self.acks.reset(AckType::AuthAck);
                    self.acks.reset(AckType::ConsensusAck);
                    self.acks.reset(AckType::NetAck);
                    self.end();
                }

                false
            }
            Resp::EndAck => {
                info!("receive restore end ack, restore end !");
                self.acks.get(AckType::AuthAck)
                    && self.acks.get(AckType::ConsensusAck)
                    && self.acks.get(AckType::NetAck)
            }
        }
        //self.send_cmd(&snap_shot);
        //false
    }

    // 发送snapshot命令
    pub fn snapshot(&self) {
        let mut req = SnapshotReq::new();
        req.set_cmd(Cmd::Snapshot);
        req.set_start_height(self.start_height);
        req.set_end_height(self.end_height);
        req.set_file(self.file.clone());
        self.send_cmd(&req);
    }

    // send begin
    pub fn begin(&self) {
        let mut req = SnapshotReq::new();
        req.set_cmd(Cmd::Begin);
        self.send_cmd(&req);
    }

    // send clear
    pub fn clear(&self) {
        let mut req = SnapshotReq::new();
        req.set_cmd(Cmd::Clear);
        self.send_cmd(&req);
    }

    // send restore
    pub fn restore(&self) {
        let mut req = SnapshotReq::new();
        req.set_cmd(Cmd::Restore);
        req.set_file(self.file.clone());
        self.send_cmd(&req);
    }

    // send end
    pub fn end(&self) {
        thread::sleep(Duration::new(5, 0));
        let mut req = SnapshotReq::new();
        req.set_cmd(Cmd::End);
        req.set_proof(self.proof.read().clone());
        req.set_end_height(self.restore_height.get());
        self.send_cmd(&req);
    }

    pub fn send_cmd(&self, snapshot_req: &SnapshotReq) {
        info!("send cmd: {:?}", snapshot_req.cmd);
        let msg: Message = snapshot_req.clone().into();
        self.ctx_pub
            .send((
                routing_key!(Snapshot >> SnapshotReq).into(),
                (&msg).try_into().unwrap(),
            ))
            .unwrap();
    }
}
