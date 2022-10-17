use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};

use actix::{Actor, Context};
use anyhow::Context as AnyhowContext;
use mediasoup::{
    consumer::Consumer,
    prelude::{ConsumerId, DtlsParameters, IceCandidate, IceParameters, ListenIp},
    producer::Producer,
    router::Router,
    rtp_parameters::RtpCapabilitiesFinalized,
    transport::TransportId,
    webrtc_transport::{TransportListenIps, WebRtcTransport, WebRtcTransportOptions},
};
use tracing::{info, instrument};

use crate::utils::{MyAddr, WrapAddr};

// First version:
// everyone connects as producer & consumer,
// stage broadcasts last person to connect.
// When a new producer joins, previous one is killed.

struct TransportOptions {
    id: TransportId,
    dtls_parameters: DtlsParameters,
    ice_candidates: Vec<IceCandidate>,
    ice_parameters: IceParameters,
}

pub mod messages {
    use super::*;
    use actix::Message;

    pub mod outgoing {
        use super::*;

        /// To user: It's your turn to be on stage.
        /// Allows them to begin streaming to a producer.
        #[derive(Message)]
        #[rtype(result = "()")]
        pub struct YourTurn {
            producer_transport_options: TransportOptions,
        }

        /// To user: Here's how to watch the show.
        /// Allows them to begin streaming from a consumer.
        #[derive(Message)]
        #[rtype(result = "()")]
        pub struct ViewParams {
            consumer_transport_options: TransportOptions,
        }
    }

    pub mod incoming {
        use mediasoup::rtp_parameters::{RtpCapabilities, RtpParameters};

        use super::*;

        /// From user: I'm ready to be on stage
        #[derive(Message)]
        #[rtype(result = "()")]
        pub struct Perform {
            pub rtp_parameters: RtpParameters,
        }

        /// From user: I want to watch the stage.
        #[derive(Message)]
        #[rtype(result = "anyhow::Result<()>")]
        pub struct Observe {
            pub dtls_parameters: DtlsParameters,
            pub rtp_capabilities: RtpCapabilities,
        }
    }

    pub mod handlers {
        use actix::{ActorFutureExt, AsyncContext, Handler, WrapFuture};
        use anyhow::anyhow;
        use mediasoup::prelude::ConsumerOptions;
        use mediasoup::{
            producer::ProducerOptions, rtp_parameters::MediaKind, transport::Transport,
        };
        use tracing::{info_span, warn};
        use tracing_actix::ActorInstrument;

        use crate::utils::LogOk;

        use super::incoming::{Observe, Perform};
        use super::*;

        impl Handler<Perform> for Stage {
            type Result = ();

            fn handle(&mut self, msg: Perform, ctx: &mut Self::Context) -> Self::Result {
                warn!("Original router id: {}", self.router.id());
                let router = self.router.clone();
                warn!("Cloned router id: {}", router.id());

                let transport_options = self.transport_options.clone();

                let pair_res_fut = async move {
                    let transport = router
                        .create_webrtc_transport(transport_options)
                        .await
                        .context("create_webrtc_transport")?;

                    // TODO: Support audio as well
                    let producer_options =
                        ProducerOptions::new(MediaKind::Video, msg.rtp_parameters);
                    let producer = transport
                        .produce(producer_options)
                        .await
                        .context("transport.produce")?;

                    let producer_pair = ProducerPair {
                        producer,
                        transport,
                    };

                    Ok::<_, anyhow::Error>(producer_pair)
                };

                let span = info_span!("follow-up: save producer");
                let actor_fut = pair_res_fut.into_actor(self).actor_instrument(span);
                let do_later = actor_fut.map(|pair_res, act, _ctx| {
                    if let Some(pair) = pair_res.ok_log_err() {
                        act.producer.replace(pair);
                    }

                    info!("Saved producer!");
                });

                ctx.spawn(do_later);
            }
        }

        impl Handler<Observe> for Stage {
            type Result = anyhow::Result<()>;

            fn handle(&mut self, msg: Observe, ctx: &mut Self::Context) -> Self::Result {
                let router = self.router.clone();
                let transport_options = self.transport_options.clone();
                let producer_id = self
                    .producer
                    .as_ref()
                    .ok_or(anyhow!("no current producer"))?
                    .producer
                    .id();

                let pair_res_fut = async move {
                    let transport = router
                        .create_webrtc_transport(transport_options)
                        .await
                        .context("create_webrtc_transport")?;

                    let consumer_options = ConsumerOptions::new(producer_id, msg.rtp_capabilities);
                    let consumer = transport
                        .consume(consumer_options)
                        .await
                        .context("transport.consume")?;

                    let consumer_pair = ConsumerPair {
                        consumer,
                        transport,
                    };

                    Ok(consumer_pair)
                };

                let span = info_span!("follow-up: save producer");
                let actor_fut = pair_res_fut.into_actor(self).actor_instrument(span);
                let do_later = actor_fut.map(|pair_res, act, _ctx| {
                    if let Some(pair) = pair_res.ok_log_err() {
                        let id = pair.consumer.id();
                        act.consumers.insert(id, pair);
                    }

                    info!("Saved producer!");
                });

                ctx.spawn(do_later);

                Ok(())
            }
        }
    }
}

struct ProducerPair {
    pub producer: Producer,
    pub transport: WebRtcTransport,
}

struct ConsumerPair {
    pub consumer: Consumer,
    pub transport: WebRtcTransport,
}

pub struct Stage {
    producer: Option<ProducerPair>,
    consumers: HashMap<ConsumerId, ConsumerPair>,
    router: Router,
    transport_options: WebRtcTransportOptions,
}

impl Stage {
    pub async fn new(router: Router) -> anyhow::Result<Self> {
        let transport_options = WebRtcTransportOptions::new(TransportListenIps::new(ListenIp {
            ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            announced_ip: None,
        }));

        let new = Self {
            producer: Default::default(),
            consumers: Default::default(),
            router,
            transport_options,
        };

        Ok(new)
    }

    pub fn get_router_rtp_capabilies(&self) -> RtpCapabilitiesFinalized {
        self.router.rtp_capabilities().clone()
    }
}

impl Actor for Stage {
    type Context = Context<Self>;

    #[instrument(skip_all)]
    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Started Stage");
    }

    #[instrument(skip_all)]
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Stopped Stage");
    }
}

pub async fn start_stage(router: Router) -> anyhow::Result<MyAddr<Stage>> {
    let stage = Stage::new(router).await.context("creating stage")?;

    let addr = stage.start().wrap();

    Ok(addr)
}
