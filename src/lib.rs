use {
    reclutch::{event, prelude::*, verbgraph as graph},
    std::{borrow::Cow, collections::HashMap},
};

#[cfg(test)]
mod tests;

type HandlerMap<T, A, E> = HashMap<&'static str, Box<dyn FnMut(&mut T, &mut A, E) + 'static>>;

/// Handles a queue, routing events into closures based on their key.
pub struct QueueHandler<T, A: 'static, E: graph::Event> {
    handlers: HandlerMap<T, A, E>,
    listener: event::RcEventListener<E>,
    node_id: NodeId,
}

impl<T, A, E: graph::Event> QueueHandler<T, A, E> {
    /// Creates a new queue handler, listening to a given node event queue.
    pub fn new<To, Ao>(node: &EventNode<To, Ao, E>) -> Self {
        QueueHandler {
            handlers: HashMap::new(),
            listener: node.queue.listen(),
            node_id: node.id,
        }
    }

    /// Adds a closure to be executed when an event of a specific key is matched.
    ///
    /// Also see [`event_key`](struct.Event.html#structmethod.get_key).
    pub fn on<'a>(
        &'a mut self,
        ev: &'static str,
        handler: impl FnMut(&mut T, &mut A, E) + 'static,
    ) -> &'a mut Self {
        self.handlers.insert(ev, Box::new(handler));
        self
    }

    /// Same as [`on`](QueueHandler::on), however `self` is consumed and returned.
    #[inline]
    pub fn and_on(
        mut self,
        ev: &'static str,
        handler: impl FnMut(&mut T, &mut A, E) + 'static,
    ) -> Self {
        self.on(ev, handler);
        self
    }

    fn handle_events(
        handlers: &mut HandlerMap<T, A, E>,
        events: &[E],
        obj: &mut T,
        additional: &mut A,
    ) {
        for event in events {
            if let Some(handler) = handlers.get_mut(event.get_key()) {
                (*handler)(obj, additional, event.clone());
            }
        }
    }
}

impl<T, A, E: graph::Event> graph::DynQueueHandler<T, A> for QueueHandler<T, A, E> {
    fn update(&mut self, obj: &mut T, additional: &mut A) {
        let handlers = &mut self.handlers;
        self.listener
            .with(move |events| Self::handle_events(handlers, events, obj, additional));
    }

    fn update_n(&mut self, n: usize, obj: &mut T, additional: &mut A) {
        let handlers = &mut self.handlers;
        self.listener.with_n(n, move |events| {
            Self::handle_events(handlers, events, obj, additional)
        });
    }
}

/// A handler convertible to [`Any`](std::any::Any).
pub trait AnyQueueHandler<T, A>: graph::DynQueueHandler<T, A> + std::any::Any {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl<X: graph::DynQueueHandler<T, A> + std::any::Any, T, A> AnyQueueHandler<T, A> for X {
    #[inline(always)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    #[inline(always)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Stores a list of queue handlers tied to nodes.
pub struct QueuedGraph<T: 'static, A: 'static> {
    handlers: HashMap<NodeId, Box<dyn AnyQueueHandler<T, A>>>,
}

impl<T: 'static, A: 'static> Default for QueuedGraph<T, A> {
    fn default() -> Self {
        QueuedGraph {
            handlers: Default::default(),
        }
    }
}

impl<T: 'static, A: 'static> QueuedGraph<T, A> {
    /// Creates a new, empty queued graph.
    /// Synonymous to [`Default::default()`].
    #[inline(always)]
    pub fn new() -> Self {
        Default::default()
    }

    /// Adds a queue handler.
    ///
    /// If the handler handles a node that has already been handled, the old handler will be replaced.
    pub fn add<'a, E: graph::Event + 'static>(
        &'a mut self,
        handler: QueueHandler<T, A, E>,
    ) -> &'a mut Self {
        self.handlers.insert(handler.node_id, Box::new(handler));
        self
    }

    /// Same as [`add`](reclutch::verbgraph::VerbGraph::add), however `self` is consumed and returned.
    #[inline]
    pub fn and_add<E: graph::Event + 'static>(mut self, handler: QueueHandler<T, A, E>) -> Self {
        self.add(handler);
        self
    }

    /// Returns an immutable reference to a queue handler for a specified node.
    #[inline]
    pub fn get_handler(&self, node: NodeId) -> Option<&dyn AnyQueueHandler<T, A>> {
        Some(self.handlers.get(&node)?.as_ref())
    }

    /// Returns an mutable reference to a queue handler for a specified node.
    #[inline]
    pub fn get_handler_mut(&mut self, node: NodeId) -> Option<&mut dyn AnyQueueHandler<T, A>> {
        Some(self.handlers.get_mut(&node)?.as_mut())
    }

    /// Invokes the queue handlers for a specific node.
    #[inline]
    pub fn update_node(&mut self, obj: &mut T, additional: &mut A, node: NodeId, length: usize) {
        if let Some(handler) = self.handlers.get_mut(&node) {
            handler.update_n(length, obj, additional);
        }
    }

    /// Returns a list of all the nodes that this graph is listening to.
    #[inline]
    pub fn subjects(&self) -> Vec<NodeId> {
        self.handlers.keys().cloned().collect()
    }
}

pub type NodeId = u64;

/// Blind record of an event from a specific node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventRecord {
    pub origin: NodeId,
}

/// Tracks event order of a singe multi-queue system.
/// You should realistically only have one instance for each thread.
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MasterNodeRecord {
    emissions: Vec<EventRecord>,
    next_id: NodeId,
}

impl MasterNodeRecord {
    /// Creates a new `MasterNodeRecord`.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns a copy of the current event record.
    #[inline]
    pub fn record(&self) -> &[EventRecord] {
        &self.emissions
    }

    fn next_id(&mut self) -> NodeId {
        self.next_id += 1;
        self.next_id - 1
    }

    fn notify(&mut self, at: usize, origin: NodeId) {
        self.emissions.insert(at + 1, EventRecord { origin });
    }

    fn notify_root(&mut self, origin: NodeId) {
        self.emissions.push(EventRecord { origin });
    }
}

/// An object containing a `QueuedGraph` and contains an `RcEventQueue`, meaning it handles both in-going and out-going events.
/// Each instance is implicitly tied to a `MasterNodeRecord`.
pub struct EventNode<T: 'static, A: 'static, E: graph::Event + 'static> {
    graph: Option<QueuedGraph<T, A>>,
    queue: event::RcEventQueue<E>,
    id: NodeId,
    current_record: Option<usize>,
    final_record: usize,
}

impl<T: 'static, A: 'static, E: graph::Event + 'static> std::ops::Deref for EventNode<T, A, E> {
    type Target = QueuedGraph<T, A>;

    #[inline]
    fn deref(&self) -> &QueuedGraph<T, A> {
        self.graph.as_ref().unwrap()
    }
}

impl<T: 'static, A: 'static, E: graph::Event + 'static> std::ops::DerefMut for EventNode<T, A, E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut QueuedGraph<T, A> {
        self.graph.as_mut().unwrap()
    }
}

impl<T: 'static, A: 'static, E: graph::Event + 'static> EventNode<T, A, E> {
    /// Creates a new `EventNode`.
    pub fn new(master_rec: &mut MasterNodeRecord) -> Self {
        EventNode {
            graph: Some(QueuedGraph::new()),
            queue: Default::default(),
            id: master_rec.next_id(),
            current_record: None,
            final_record: 0,
        }
    }

    /// The unique ID of this node.
    #[inline]
    pub fn id(&self) -> NodeId {
        self.id
    }
}

impl<T: 'static, A: 'static, E: graph::Event + 'static> QueueInterfaceCommon
    for EventNode<T, A, E>
{
    type Item = E;
}

impl<T: 'static, A: 'static, E: graph::Event + Clone + 'static> QueueInterfaceListable
    for EventNode<T, A, E>
{
    type Listener = event::RcEventListener<E>;

    #[inline]
    fn listen(&self) -> event::RcEventListener<E> {
        self.queue.listen()
    }
}

impl<T: 'static, A: 'static, E: graph::Event + 'static> EventNode<T, A, E> {
    /// Notifies the master event record and forwards event to the inner queue.
    pub fn emit<'a>(
        &mut self,
        event: Cow<'a, E>,
        mnr: &mut MasterNodeRecord,
    ) -> event::EmitResult<'a, E> {
        if let Some(cr) = &mut self.current_record {
            // this event happened as a result to another, thus it must have happened *after* the current event.
            mnr.notify(*cr, self.id);
            *cr += 1;
        } else {
            // this event happened "in root". no event occurred to cause this
            mnr.notify_root(self.id);
        }

        self.queue.emit(event)
    }

    /// Convenience wrapper around `emit`.
    /// Takes an owned event and emits it as a `Cow::Owned`.
    #[inline]
    pub fn emit_owned<'a>(
        &mut self,
        event: E,
        mnr: &mut MasterNodeRecord,
    ) -> event::EmitResult<'a, E> {
        self.emit(Cow::Owned(event), mnr)
    }

    /// Convenience wrapper around `emit`.
    /// Takes a borrowed event and emits it as a `Cow::Borrowed`.
    #[inline]
    pub fn emit_borrowed<'a>(
        &mut self,
        event: &'a E,
        mnr: &mut MasterNodeRecord,
    ) -> event::EmitResult<'a, E> {
        self.emit(Cow::Borrowed(event), mnr)
    }

    /// Removes the inner graph and returns it.
    /// This is necessary for handling ownership when updating.
    ///
    /// **Always** call `reset` when finished.
    #[inline]
    pub fn take(&mut self) -> QueuedGraph<T, A> {
        self.graph.take().unwrap()
    }

    /// Resets the `graph` that was `taken`.
    ///
    /// Invoke responsibly; only reset what was returned from `take`.
    #[inline]
    pub fn reset(&mut self, graph: QueuedGraph<T, A>) {
        self.graph = Some(graph);
    }

    /// Sets the current event record index, if any.
    ///
    /// The current event record keeps track of the current event
    /// being processed, so that if an event is emitted within a handler,
    /// the emitted event can be inserted into the master record after
    /// the handled event.
    #[inline]
    pub fn set_record(&mut self, record: Option<usize>) {
        self.current_record = record;
    }

    /// Sets the latest final record.
    #[inline]
    pub fn set_final_record(&mut self, final_rec: usize) {
        self.final_record = final_rec;
    }

    /// Returns the latest final record.
    ///
    /// The final record keeps track of the final index reached after `update` is finished.
    /// This is so that event records aren't reprocessed
    #[inline]
    pub fn final_record(&self) -> usize {
        self.final_record
    }
}

/// Genericized updater which follows the master event order.
///
/// Invoker should be a function which invokes the `update_n` function.
/// The named signature of this is `FnMut(item, current_record, length) -> new_records`.
// NOTE(zserik) `length` is the event count, right?
///
/// A general implementation may look like this;
/// ```ignore
/// let invoker = |obj: &mut Object, node: NodeId, current_record: usize, length: usize| -> Vec<EventRecord> {
///     obj.node.set_record(Some(current_record));
///     let mut graph = obj.graph.take();
///     // &mut aux comes from the environment of the closure
///     graph.update_node(obj, &mut aux, node, length);
///     obj.node.reset(graph);
///     obj.node.set_record(None);
///     aux.master.record()
/// };
/// ```
///
/// Critical things that `invoker` must do;
/// - Update `current_record` of graph.
/// - Correctly `take` and `reset` inner `VerbGraph` of graph in order to call `update_n`.
/// - Invoke `update_n` on graph, using `length`.
/// - Reset `current_record` after `update_n`.
/// - Return new records.
///
/// Indexer should be a function which returns all the graphs that a given `T` is listening to.
/// The implementation should invariably lead to an invocation to `QueuedGraph::subjects`.
///
/// A general implementation may look like this;
/// ```ignore
/// fn indexer(obj: &Object) -> Vec<GraphId> {
///     obj.node.subjects()
/// }
/// ```
///
/// Recorder should return the final record.
///
/// Finalizer should update the final record.
pub fn update<T>(
    items: &mut [T],
    mut rec: Vec<EventRecord>,
    mut invoker: impl FnMut(&mut T, NodeId, usize, usize) -> Vec<EventRecord>,
    indexer: impl Fn(&T) -> Vec<NodeId>,
    recorder: impl Fn(&T) -> usize,
    finalizer: impl Fn(&mut T, usize),
) {
    if items.is_empty() {
        return;
    }

    let mut highest_common_rec = std::usize::MAX;
    let mut index: HashMap<NodeId, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let (graphs, final_rec) = (indexer(item), recorder(item));
        highest_common_rec = highest_common_rec.min(final_rec);
        for graph in graphs {
            index.entry(graph).or_default().push(i);
        }
    }

    if highest_common_rec == rec.len() {
        return;
    }

    let mut i = highest_common_rec;
    let mut emit_count = rec.len();
    while i < emit_count {
        if let Some(indices) = index.get(&rec[i].origin) {
            for idx in indices {
                if i < recorder(&items[*idx]) {
                    // this item has already processed this event.
                    // reprocessing it would be a bug
                    continue;
                }
                let new_rec = invoker(&mut items[*idx], rec[i].origin, i, 1);
                rec = new_rec;
                emit_count = rec.len();
            }
        }
        i += 1;
    }

    for item in items {
        finalizer(item, emit_count);
    }
}
