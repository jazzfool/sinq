#[cfg_attr(test, macro_use)]
pub extern crate reclutch;

use {
    reclutch::{event, prelude::*, verbgraph as graph},
    std::{borrow::Cow, cell::RefCell, collections::HashMap, rc::Rc},
};

pub struct QueueHandler<T, A: 'static, E: graph::Event> {
    handlers: HashMap<&'static str, Rc<RefCell<dyn FnMut(&mut T, &mut A, E)>>>,
    listener: event::RcEventListener<E>,
    node_id: NodeId,
}

impl<T, A, E: graph::Event> QueueHandler<T, A, E> {
    /// Creates a new queue handler, listening to a given event queue.
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
        self.handlers.insert(ev, Rc::new(RefCell::new(handler)));
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
}

impl<T, A, E: graph::Event> graph::DynQueueHandler<T, A> for QueueHandler<T, A, E> {
    fn update(&mut self, obj: &mut T, additional: &mut A) {
        let handlers = &mut self.handlers;
        self.listener.with(|events| {
            for event in events {
                if let Some(handler) = handlers.get_mut(event.get_key()) {
                    use std::ops::DerefMut;
                    let mut handler = handler.as_ref().borrow_mut();
                    (handler.deref_mut())(obj, additional, event.clone());
                }
            }
        });
    }

    fn update_n(&mut self, n: usize, obj: &mut T, additional: &mut A) {
        let handlers = &mut self.handlers;
        self.listener.with_n(n, |events| {
            for event in events {
                if let Some(handler) = handlers.get_mut(event.get_key()) {
                    use std::ops::DerefMut;
                    let mut handler = handler.as_ref().borrow_mut();
                    (handler.deref_mut())(obj, additional, event.clone());
                }
            }
        });
    }
}

pub struct QueuedGraph<T: 'static, A: 'static> {
    handlers: HashMap<NodeId, Box<dyn graph::DynQueueHandler<T, A>>>,
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
    /// Synonymous to `Default::default()`.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Adds a queue handler.
    pub fn add<'a, E: graph::Event + 'static>(
        &'a mut self,
        handler: QueueHandler<T, A, E>,
    ) -> &'a mut Self {
        self.handlers
            .entry(handler.node_id)
            .or_insert(Box::new(handler));
        self
    }

    /// Same as [`add`](VerbGraph::add), however `self` is consumed and returned.
    #[inline]
    pub fn and_add<E: graph::Event + 'static>(mut self, handler: QueueHandler<T, A, E>) -> Self {
        self.add(handler);
        self
    }

    fn update_handler(
        handler: &mut dyn graph::DynQueueHandler<T, A>,
        obj: &mut T,
        additional: &mut A,
    ) {
        handler.update(obj, additional);
    }

    /// Invokes all the queue handlers in a linear fashion, however non-linear jumping between verb graphs is still supported.
    pub fn update_all(&mut self, obj: &mut T, additional: &mut A) {
        for handler in self.handlers.values_mut() {
            QueuedGraph::update_handler(handler.as_mut(), obj, additional)
        }
    }

    /// Invokes the queue handlers for a specific node.
    #[inline]
    pub fn update_node(&mut self, obj: &mut T, additional: &mut A, node: NodeId, length: usize) {
        if let Some(handler) = self.handlers.get_mut(&node) {
            handler.update_n(length, obj, additional);
        }
    }

    #[inline]
    pub fn subjects(&self) -> Vec<NodeId> {
        self.handlers.keys().cloned().collect()
    }
}

pub type NodeId = u64;

#[derive(Debug, Clone)]
pub struct EventRecord {
    pub origin: NodeId,
}

#[derive(Default)]
pub struct MasterNodeRecord {
    emissions: Vec<EventRecord>,
    next_id: NodeId,
}

impl MasterNodeRecord {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns a copy of the current event record.
    #[inline]
    pub fn record(&self) -> Vec<EventRecord> {
        self.emissions.clone()
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

pub struct EventNode<T: 'static, A: 'static, E: graph::Event + 'static> {
    graph: Option<QueuedGraph<T, A>>,
    queue: event::RcEventQueue<E>,
    id: NodeId,
    current_record: Option<usize>,
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
    pub fn new(master_rec: &mut MasterNodeRecord) -> Self {
        EventNode {
            graph: Some(QueuedGraph::new()),
            queue: Default::default(),
            id: master_rec.next_id(),
            current_record: None,
        }
    }

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

    #[inline]
    pub fn emit_owned<'a>(
        &mut self,
        event: E,
        mnr: &mut MasterNodeRecord,
    ) -> event::EmitResult<'a, E> {
        self.emit(Cow::Owned(event), mnr)
    }

    #[inline]
    pub fn emit_borrowed<'a>(
        &mut self,
        event: &'a E,
        mnr: &mut MasterNodeRecord,
    ) -> event::EmitResult<'a, E> {
        self.emit(Cow::Borrowed(event), mnr)
    }

    #[inline]
    pub fn take(&mut self) -> QueuedGraph<T, A> {
        self.graph.take().unwrap()
    }

    #[inline]
    pub fn reset(&mut self, graph: QueuedGraph<T, A>) {
        self.graph = Some(graph);
    }

    #[inline]
    pub fn set_record(&mut self, record: Option<usize>) {
        self.current_record = record;
    }
}

/// Genericized updater which follows the master event order.
///
/// Invoker should be a function which invokes the `update_n` function.
/// The named signature of this is `Fn(item, aux, current_record, length) -> (new_record, count)`.
///
/// A general implementation may look like this;
/// ```ignore
/// fn invoker(obj: &mut Object, aux: &mut Aux, node: NodeId, current_record: usize, length: usize) -> u32 {
///     obj.node.reset_count();
///     obj.node.set_record(Some(current_record));
///     let mut graph = obj.graph.take();
///     graph.update_node(obj, aux, node, length);
///     obj.node.reset(graph);
///     obj.node.set_record(None);
///     (aux.master.record(), obj.node.count())
/// }
/// ```
///
/// Critical things that `invoker` must do;
/// - Reset emission count;
/// - Update `current_record` of graph.
/// - Correctly `take` and `reset` inner `VerbGraph` of graph in order to call `update_n`.
/// - Invoke `update_n` on graph, using `length`.
/// - Reset `current_record` after `update_n`.
/// - Return emission count.
/// - Return new record;
///
/// Indexer should be a function which returns all the graphs that a given `T` is listening to.
/// The implementation should invariably lead to an invocation to `QueuedGraph::subjects`.
///
/// A general implementation may look like this;
/// ```ignore
/// fn indexer(obj: &mut Object) -> Vec<GraphId> {
///     obj.node.subjects() // through Deref<Target = QueuedGraph>
/// }
/// ```
pub fn update<T, A: 'static>(
    items: &mut [T],
    aux: &mut A,
    mut rec: Vec<EventRecord>,
    invoker: impl Fn(&mut T, &mut A, NodeId, usize, usize) -> Vec<EventRecord>,
    indexer: impl Fn(&T) -> Vec<NodeId>,
) {
    let mut index: HashMap<NodeId, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let graphs = indexer(item);
        for graph in graphs {
            index.entry(graph).or_default().push(i);
        }
    }

    let mut i = 0;
    let mut emit_count = rec.len();
    while i < emit_count {
        if let Some(indices) = index.get(&rec[i].origin) {
            for idx in indices {
                let new_rec = invoker(&mut items[*idx], aux, rec[i].origin, i, 1);
                rec = new_rec;
                emit_count = rec.len();
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait ObjectNode {
        fn node_subjects(&self) -> Vec<NodeId>;
        fn update(
            &mut self,
            master: &mut MasterNodeRecord,
            node: NodeId,
            current_rec: usize,
            length: usize,
        ) -> Vec<EventRecord>;
    }

    struct Object<E: graph::Event + 'static>(
        EventNode<Self, MasterNodeRecord, E>,
        Vec<&'static str>,
    );

    impl<E: graph::Event + 'static> Object<E> {
        fn new(master: &mut MasterNodeRecord) -> Self {
            Object(EventNode::new(master), Default::default())
        }
    }

    impl<E: graph::Event + 'static> ObjectNode for Object<E> {
        #[inline]
        fn node_subjects(&self) -> Vec<NodeId> {
            self.0.subjects()
        }

        fn update(
            &mut self,
            master: &mut MasterNodeRecord,
            node: NodeId,
            current_rec: usize,
            length: usize,
        ) -> Vec<EventRecord> {
            self.0.set_record(Some(current_rec));
            let mut graph = self.0.take();
            graph.update_node(self, master, node, length);
            self.0.reset(graph);
            self.0.set_record(None);
            master.record()
        }
    }

    #[derive(Clone, Event)]
    enum EventA {
        #[event_key(aa)]
        A,
        #[event_key(ab)]
        B,
    }

    #[derive(Clone, Event)]
    #[event_key(b)]
    struct EventB;

    #[derive(Clone, Event)]
    #[event_key(none)]
    struct NoEvent;

    #[test]
    fn test_master_record() {
        let mut master = MasterNodeRecord::new();

        let mut obj_0 = Object::<EventA>::new(&mut master);
        let mut obj_1 = Object::<NoEvent>::new(&mut master);
        let mut obj_2 = Object::<EventB>::new(&mut master);
        let mut obj_3 = Object::<EventB>::new(&mut master);

        obj_2.0.add(QueueHandler::new(&obj_0.0).and_on(
            "aa",
            |o: &mut Object<EventB>, mnr: &mut MasterNodeRecord, _| {
                o.0.emit_owned(EventB, mnr);
            },
        ));

        obj_3.0.add(
            QueueHandler::new(&obj_0.0).and_on("ab", |o: &mut Object<EventB>, mnr, _| {
                o.0.emit_owned(EventB, mnr);
            }),
        );

        obj_1.0.add(
            QueueHandler::new(&obj_2.0).and_on("b", |o: &mut Object<NoEvent>, _, _| {
                o.1.push("obj_2");
            }),
        );

        obj_1.0.add(
            QueueHandler::new(&obj_3.0).and_on("b", |o: &mut Object<NoEvent>, _, _| {
                o.1.push("obj_3");
            }),
        );

        // the order of events emitted should be the only deciding factor of the final order of events received by obj_1.
        obj_0.0.emit_owned(EventA::B, &mut master);
        obj_0.0.emit_owned(EventA::A, &mut master);
        obj_0.0.emit_owned(EventA::A, &mut master);
        obj_0.0.emit_owned(EventA::B, &mut master);
        obj_0.0.emit_owned(EventA::A, &mut master);

        // object list put in "worst-case" order.
        // obj_1 should be checked after obj_2 and obj_3.
        // obj_3 should be checked before obj_2.
        // if implemented correctly, none of this will matter, it'll sort it correctly.
        let objs: &mut [&mut dyn ObjectNode] = &mut [&mut obj_1, &mut obj_2, &mut obj_3];
        let record = master.record();

        update(
            objs,
            &mut master,
            record,
            |obj: &mut &mut dyn ObjectNode, aux, node, current_rec, length| -> Vec<EventRecord> {
                obj.update(aux, node, current_rec, length)
            },
            |obj| obj.node_subjects(),
        );

        assert_eq!(&obj_1.1, &["obj_3", "obj_2", "obj_2", "obj_3", "obj_2"]);
    }
}
