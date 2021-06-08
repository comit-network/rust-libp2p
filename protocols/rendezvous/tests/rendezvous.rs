pub mod harness;

use crate::harness::{await_events_or_timeout, new_swarm, SwarmExt};
use libp2p_rendezvous::behaviour::{Event, Rendezvous};
use libp2p_swarm::Swarm;
use std::time::Duration;

#[tokio::test]
async fn given_successful_registration_then_successful_discovery() {
    env_logger::init();
    let mut test = RendezvousTest::setup().await;

    let namespace = "some-namespace".to_string();

    println!("registring");

    // register
    test.registration_swarm.behaviour_mut().register(
        namespace.clone(),
        test.rendezvous_swarm.local_peer_id().clone(),
    );

    test.assert_successful_registration(namespace.clone()).await;

    println!("Registration worked!");

    // // discover
    // test.discovery_swarm
    //     .behaviour_mut()
    //     .discover(Some(namespace), test.rendezvous_peer_id);
    //
    // test.assert_successful_discovery().await;
}

struct RendezvousTest {
    pub registration_swarm: Swarm<Rendezvous>,
    pub discovery_swarm: Swarm<Rendezvous>,
    pub rendezvous_swarm: Swarm<Rendezvous>,
}

impl RendezvousTest {
    pub async fn setup() -> Self {
        let mut registration_swarm =
            new_swarm(|_, identity| Rendezvous::new(identity, "Registration".to_string()));
        registration_swarm.listen_on_random_memory_address().await;

        let mut discovery_swarm =
            new_swarm(|_, identity| Rendezvous::new(identity, "Discovery".to_string()));
        discovery_swarm.listen_on_random_memory_address().await;

        let mut rendezvous_swarm =
            new_swarm(|_, identity| Rendezvous::new(identity, "Rendezvous".to_string()));
        rendezvous_swarm.listen_on_random_memory_address().await;

        registration_swarm
            .block_on_connection(&mut rendezvous_swarm)
            .await;
        discovery_swarm
            .block_on_connection(&mut rendezvous_swarm)
            .await;

        Self {
            registration_swarm,
            discovery_swarm,
            rendezvous_swarm,
        }
    }

    pub async fn assert_successful_registration(&mut self, namespace: String) {
        match await_events_or_timeout(self.rendezvous_swarm.next(), self.registration_swarm.next()).await {
            (
                Event::PeerRegistered { .. },
                Event::RegisteredWithRendezvousNode { .. },
            ) => {
            }
            (rendezvous_swarm_event, registration_swarm_event) => panic!(
                "Received unexpected event, rendezvous swarm emitted {:?} and registration swarm emitted {:?}",
                rendezvous_swarm_event, registration_swarm_event
            ),
        }
    }

    pub async fn assert_successful_discovery(&mut self) {
        // TODO: Is it by design that there is no event emitted on the rendezvous side for discovery?

        match tokio::time::timeout(Duration::from_secs(10), self.discovery_swarm.next())
            .await
            .expect("")
        {
            // TODO: Assert against the actual registration
            Event::Discovered { .. } => {}
            event => panic!("Discovery swarm emitted unexpected event {:?}", event),
        }
    }
}
