use {
    super::*,
    reclutch::{verbgraph as graph, Event},
};

trait ObjectNode {
    fn node_subjects(&self) -> Vec<NodeId>;
    fn update(
        &mut self,
        master: &mut MasterNodeRecord,
        node: NodeId,
        current_rec: usize,
        length: usize,
    ) -> Vec<EventRecord>;
    fn node_final(&self) -> usize;
    fn node_set_final(&mut self, fr: usize);
}

struct Object<E: graph::Event + 'static>(EventNode<Self, MasterNodeRecord, E>, Vec<&'static str>);

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
        master.record().to_vec()
    }

    fn node_final(&self) -> usize {
        self.0.final_record()
    }

    fn node_set_final(&mut self, fr: usize) {
        self.0.set_final_record(fr);
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
    let record = master.record().to_vec();

    update(
        objs,
        record,
        |obj: &mut &mut dyn ObjectNode, node, current_rec, length| -> Vec<EventRecord> {
            obj.update(&mut master, node, current_rec, length)
        },
        |obj| obj.node_subjects(),
        |obj| obj.node_final(),
        |obj, fr| obj.node_set_final(fr),
    );

    assert_eq!(&obj_1.1, &["obj_3", "obj_2", "obj_2", "obj_3", "obj_2"]);
}
