# Sinq

## Synchronised event queues

Based upon `reclutch`, this library introduces a master record of the order of which events were emitted. This can then be used to update event listeners in the correct order.

## What does it do?

Suppose you have 3 objects containing an event queue and event listener; A, B and C.

B and C both listen to A and in response push an event into their respective queues.
Assume that B only responds to event X and C only responds to event Y.

If A emits two events, X and Y, in that order, the expected order is for B and C to emit *their* events, in that order.

What if, however, C checks for events from A before B? C doesn't know about B or event X, it only cares about event Y, and as such has no concept that Y comes after X and therefore must wait it's turn before responding.

This is what leads to out-of-order events - a problem that occurs due to having a multi-queue system.

Sinq makes the aforementioned scenario impossible simply by keeping track of the order of events and using it to update the correct instances in the correct order.

For example, using the scenario already mentioned, instead of the application manually updating B and C, B and C are given to Sinq and Sinq will find the correct order and invoke updates accordingly.

## License

Sinq is licensed under either

- [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0)
- [MIT](http://opensource.org/licenses/MIT)

at your choosing.
