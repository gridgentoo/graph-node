use futures::sync::mpsc::{channel, Receiver, Sender};
use std::collections::HashSet;
use std::sync::Mutex;

use graph::data::subgraph::schema::attribute_index_definitions;
use graph::prelude::{SubgraphAssignmentProvider as SubgraphAssignmentProviderTrait, *};

pub struct SubgraphAssignmentProvider<L, S> {
    logger: Logger,
    event_stream: Option<Receiver<SubgraphAssignmentProviderEvent>>,
    event_sink: Sender<SubgraphAssignmentProviderEvent>,
    resolver: Arc<L>,
    subgraphs_running: Arc<Mutex<HashSet<SubgraphDeploymentId>>>,
    store: Arc<S>,
}

impl<L, S> SubgraphAssignmentProvider<L, S>
where
    L: LinkResolver,
    S: Store,
{
    pub fn new(logger: Logger, resolver: Arc<L>, store: Arc<S>) -> Self {
        let (event_sink, event_stream) = channel(100);

        // Create the subgraph provider
        SubgraphAssignmentProvider {
            logger: logger.new(o!("component" => "SubgraphAssignmentProvider")),
            event_stream: Some(event_stream),
            event_sink,
            resolver,
            subgraphs_running: Arc::new(Mutex::new(HashSet::new())),
            store,
        }
    }

    /// Clones but forcing receivers to `None`.
    fn clone(&self) -> Self {
        SubgraphAssignmentProvider {
            logger: self.logger.clone(),
            event_stream: None,
            event_sink: self.event_sink.clone(),
            resolver: self.resolver.clone(),
            subgraphs_running: self.subgraphs_running.clone(),
            store: self.store.clone(),
        }
    }
}

impl<L, S> SubgraphAssignmentProviderTrait for SubgraphAssignmentProvider<L, S>
where
    L: LinkResolver,
    S: Store,
{
    fn start(
        &self,
        id: SubgraphDeploymentId,
    ) -> Box<Future<Item = (), Error = SubgraphAssignmentProviderError> + Send + 'static> {
        let self_clone = self.clone();
        let store = self.store.clone();
        let subgraph_id = id.clone();

        let link = format!("/ipfs/{}", id);

        Box::new(
            SubgraphManifest::resolve(Link { link }, self.resolver.clone())
                .map_err(SubgraphAssignmentProviderError::ResolveError)
                .and_then(move |subgraph| -> Box<Future<Item = _, Error = _> + Send> {
                    // If subgraph ID already in set
                    if !self_clone
                        .subgraphs_running
                        .lock()
                        .unwrap()
                        .insert(subgraph.id.clone())
                    {
                        return Box::new(future::err(
                            SubgraphAssignmentProviderError::AlreadyRunning(subgraph.id),
                        ));
                    }

                    // Build indexes for each entity attribute in the Subgraph
                    let index_definitions = attribute_index_definitions(
                        subgraph.id.clone(),
                        subgraph.schema.document.clone(),
                    );
                    self_clone
                        .store
                        .clone()
                        .build_entity_attribute_indexes(index_definitions)
                        .map(|_| {
                            info!(
                                self_clone.logger,
                                "Successfully created attribute indexes for subgraph entities"
                            )
                        })
                        .ok();

                    // Send events to trigger subgraph processing
                    Box::new(
                        self_clone
                            .event_sink
                            .clone()
                            .send(SubgraphAssignmentProviderEvent::SubgraphStart(subgraph))
                            .map_err(|e| panic!("failed to forward subgraph: {}", e))
                            .map(|_| ()),
                    )
                })
                .map_err(move |e| {
                    let _ = store.apply_entity_operations(
                        SubgraphDeploymentEntity::update_failed_operations(&subgraph_id, true),
                        EventSource::None,
                    );
                    e
                }),
        )
    }

    fn stop(
        &self,
        id: SubgraphDeploymentId,
    ) -> Box<Future<Item = (), Error = SubgraphAssignmentProviderError> + Send + 'static> {
        // If subgraph ID was in set
        if self.subgraphs_running.lock().unwrap().remove(&id) {
            // Shut down subgraph processing
            Box::new(
                self.event_sink
                    .clone()
                    .send(SubgraphAssignmentProviderEvent::SubgraphStop(id))
                    .map_err(|e| panic!("failed to forward subgraph shut down event: {}", e))
                    .map(|_| ()),
            )
        } else {
            Box::new(future::err(SubgraphAssignmentProviderError::NotRunning(id)))
        }
    }
}

impl<L, S> EventProducer<SubgraphAssignmentProviderEvent> for SubgraphAssignmentProvider<L, S> {
    fn take_event_stream(
        &mut self,
    ) -> Option<Box<Stream<Item = SubgraphAssignmentProviderEvent, Error = ()> + Send>> {
        self.event_stream.take().map(|s| {
            Box::new(s) as Box<Stream<Item = SubgraphAssignmentProviderEvent, Error = ()> + Send>
        })
    }
}
