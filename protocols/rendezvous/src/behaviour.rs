use crate::codec::{ErrorCode, Message, Registration};
use crate::handler::{Input, RendezvousHandler};
use libp2p_core::connection::ConnectionId;
use libp2p_core::{AuthenticatedPeerRecord, Multiaddr, PeerId};
use libp2p_swarm::{
    NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, PollParameters, ProtocolsHandler,
};
use log::debug;
use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll};

pub struct Rendezvous {
    events: VecDeque<NetworkBehaviourAction<Input, Event>>,
    registrations: HashMap<String, HashSet<Registration>>,
}

impl Rendezvous {
    pub fn new() -> Self {
        Self {
            events: Default::default(),
            registrations: Default::default(),
        }
    }

    pub fn register(
        &mut self,
        ns: String,
        rendezvous_node: PeerId,
        record: AuthenticatedPeerRecord,
    ) {
        self.events
            .push_back(NetworkBehaviourAction::NotifyHandler {
                peer_id: rendezvous_node,
                event: Input::RegisterRequest {
                    namespace: ns,
                    ttl: None,
                    record,
                },
                handler: NotifyHandler::Any,
            });
    }

    pub fn unregister(&mut self, ns: String, rendezvous_node: PeerId) {
        self.events
            .push_back(NetworkBehaviourAction::NotifyHandler {
                peer_id: rendezvous_node,
                event: Input::UnregisterRequest { namespace: ns },
                handler: NotifyHandler::Any,
            });
    }
    pub fn discover(&mut self, ns: Option<String>, rendezvous_node: PeerId) {
        self.events
            .push_back(NetworkBehaviourAction::NotifyHandler {
                peer_id: rendezvous_node,
                event: Input::DiscoverRequest { namespace: ns },
                handler: NotifyHandler::Any,
            });
    }
}

#[derive(Debug)]
pub enum Event {
    Discovered {
        rendezvous_node: PeerId,
        ns: Vec<Registration>,
    },
    FailedToDiscover {
        rendezvous_node: PeerId,
        err_code: ErrorCode,
    },
    RegisteredWithRendezvousNode {
        rendezvous_node: PeerId,
        ns: String,
        ttl: i64,
    },
    FailedToRegisterWithRendezvousNode {
        rendezvous_node: PeerId,
        ns: String,
        err_code: ErrorCode,
    },
    DeclinedRegisterRequest {
        peer: PeerId,
        ns: String,
    },
    PeerRegistered {
        peer_id: PeerId,
        ns: String,
    },
    PeerUnregistered {
        peer_id: PeerId,
        ns: String,
    },
}

impl NetworkBehaviour for Rendezvous {
    type ProtocolsHandler = RendezvousHandler;
    type OutEvent = Event;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        RendezvousHandler::new()
    }

    fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, peer_id: &PeerId) {
        debug!("New peer connected: {}", peer_id);
        // Dont need to do anything here?
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId) {
        debug!("Peer disconnected: {}", peer_id);
        // Don't need to do anything?
    }

    fn inject_event(
        &mut self,
        peer_id: PeerId,
        _connection: ConnectionId,
        event: crate::handler::HandlerEvent,
    ) {
        match event.0 {
            Message::Register(new_reggo) => {
                let ttl = new_reggo.effective_ttl();

                self.registrations
                    .entry(new_reggo.namespace)
                    .or_insert_with(|| HashSet::new())
                    .insert(new_reggo.clone());

                self.events
                    .push_back(NetworkBehaviourAction::NotifyHandler {
                        peer_id,
                        handler: NotifyHandler::Any,
                        event: Input::RegisterResponse {
                            ttl,
                            message: Message::SuccessfullyRegistered { ttl },
                        },
                    })
            }
            Message::SuccessfullyRegistered { ttl } => {
                // where to get namespace from?
                self.events.push_back(NetworkBehaviourAction::GenerateEvent(
                    Event::RegisteredWithRendezvousNode {
                        rendezvous_node: peer_id,
                        ns: "".to_string(),
                        ttl,
                    },
                ))
            }
            Message::FailedToRegister { error } => {
                self.events.push_back(NetworkBehaviourAction::GenerateEvent(
                    Event::FailedToRegisterWithRendezvousNode {
                        rendezvous_node: peer_id,
                        // todo: need to get the namespace somehow? The handler will probably have to remember
                        // the request this message is a response to as the wire message does not contain this info
                        ns: "".to_string(),
                        err_code: error,
                    },
                ))
            }
            Message::Unregister { namespace } => {
                if let Some(registrations) = self.registrations.get_mut(&namespace) {
                    if registrations.contains(&peer_id) {
                        registrations.remove(&peer_id);
                    }
                }
                // todo: maybe send a unregister response to the remote?
            }
            Message::Discover { namespace } => {
                if let Some(ns) = namespace {
                    if let Some(peers) = self.registrations.get_mut(&ns) {
                        self.events
                            .push_back(NetworkBehaviourAction::NotifyHandler {
                                peer_id,
                                handler: NotifyHandler::Any,
                                event: Input::DiscoverResponse {
                                    registrations: peers.iter().map(|r| r.clone()).collect(),
                                },
                            });
                    }
                } else {
                    let discovered = self
                        .registrations
                        .iter()
                        .map(|(ns, registrations)| {
                            registrations
                                .iter()
                                .map(|registration| registration.clone())
                                .collect::<Vec<Registration>>()
                                .into_iter()
                        })
                        .flatten()
                        .collect::<Vec<Registration>>();

                    self.events
                        .push_back(NetworkBehaviourAction::NotifyHandler {
                            peer_id,
                            handler: NotifyHandler::Any,
                            event: Input::DiscoverResponse {
                                registrations: discovered,
                            },
                        });
                }
                self.events
                    .push_back(NetworkBehaviourAction::NotifyHandler {
                        peer_id,
                        handler: NotifyHandler::Any,
                        event: Input::DiscoverResponse {
                            registrations: vec![],
                        },
                    })
            }
            Message::DiscoverResponse { registrations } => {
                self.events
                    .push_back(NetworkBehaviourAction::GenerateEvent(Event::Discovered {
                        rendezvous_node: peer_id,
                        ns: registrations,
                    }))
            }
            Message::FailedToDiscover { error } => self.events.push_back(
                NetworkBehaviourAction::GenerateEvent(Event::FailedToDiscover {
                    rendezvous_node: peer_id,
                    err_code: error,
                }),
            ),
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
        _: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            <Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}
