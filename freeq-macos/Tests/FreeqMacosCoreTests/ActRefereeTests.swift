import XCTest
@testable import FreeqMacosCore

final class ActRefereeTests: XCTestCase {
    private let home = "did:web:referee.example"

    /// A gate over a log of applied ids, with each task's opener as held,
    /// and the timers it set, to fire by hand.
    private final class Rig {
        var applied: [String] = []
        var openers: [String: [String: String]] = [:]
        var timers: [() -> Void] = []
        lazy var gate = RulingGate<Void>(
            refereeOf: { [unowned self] taskId in
                guard let fields = self.openers[taskId] else { return .noOpener }
                return .named(fields["act-home"])
            },
            apply: { [unowned self] ev in
                self.applied.append(ev.eventId)
                if ev.eventId == ev.taskId { self.openers[ev.taskId] = ev.fields }
            },
            schedule: { [unowned self] run in self.timers.append(run) })
    }

    private func opener(_ taskId: String, home: String? = nil) -> GatedAct<Void> {
        GatedAct(taskId: taskId, eventId: taskId, verb: "offer", did: "did:plc:poster",
                 fields: home.map { ["act-home": $0] } ?? [:], verdict: nil, payload: ())
    }

    private func move(_ taskId: String, _ eventId: String, _ verb: String, _ did: String,
                      _ verdict: String? = nil) -> GatedAct<Void> {
        GatedAct(taskId: taskId, eventId: eventId, verb: verb, did: did, fields: [:],
                 verdict: verdict, payload: ())
    }

    func testARulingWaitsForItsVerdictAndLandsInItsPlace() {
        let r = Rig()
        r.gate.offer(opener("T1", home: home))
        r.gate.offer(move("T1", "R1", "confirm", home, "pending"))
        r.gate.offer(move("T1", "P1", "progress", "did:plc:worker"))
        XCTAssertEqual(r.applied, ["T1"])
        r.gate.settle(eventId: "R1", verdict: "device")
        XCTAssertEqual(r.applied, ["T1", "R1", "P1"])
    }

    func testARulingCountsOnTheRefereesDeviceOrServerSignature() {
        for state in ["device", "server"] {
            let r = Rig()
            r.gate.offer(opener("T2", home: home))
            r.gate.offer(move("T2", "R2", "expire", home, state))
            XCTAssertEqual(r.applied, ["T2", "R2"], state)
        }
    }

    func testARulingThatFailsItsCheckChangesNothing() {
        for state in ["invalid", "retired"] {
            let r = Rig()
            r.gate.offer(opener("T3", home: home))
            r.gate.offer(move("T3", "R3", "expire", home, "pending"))
            r.gate.offer(move("T3", "P3", "progress", "did:plc:worker"))
            r.gate.settle(eventId: "R3", verdict: state)
            XCTAssertEqual(r.applied, ["T3", "P3"], state)
        }
    }

    func testARulingAnotherServerSignedChangesNothing() {
        let r = Rig()
        r.gate.offer(opener("T4", home: home))
        r.gate.offer(move("T4", "R4", "confirm", "did:web:elsewhere.example", "device"))
        XCTAssertEqual(r.applied, ["T4"])
    }

    func testARulingWithNoCheckableVerdictAppliesAsToday() {
        for state in ["unverifiable", "unsigned", nil] {
            let r = Rig()
            r.gate.offer(opener("T5", home: home))
            r.gate.offer(move("T5", "R5", "auto-accept", home, state))
            XCTAssertEqual(r.applied, ["T5", "R5"], String(describing: state))
        }
    }

    func testARulingWhoseVerdictNeverSettlesAppliesAsToday() {
        let r = Rig()
        r.gate.offer(opener("T6", home: home))
        r.gate.offer(move("T6", "R6", "expire", home, "pending"))
        XCTAssertEqual(r.applied, ["T6"])
        XCTAssertEqual(r.timers.count, 1)
        r.timers[0]()
        XCTAssertEqual(r.applied, ["T6", "R6"])
    }

    func testARulingWaitsForItsTasksOpener() {
        let r = Rig()
        r.gate.offer(move("T7", "R7", "expire", "did:web:elsewhere.example", "device"))
        r.gate.offer(move("T8", "R8", "expire", home, "device"))
        XCTAssertEqual(r.applied, [])
        r.gate.offer(opener("T7", home: home))
        r.gate.offer(opener("T8", home: home))
        XCTAssertEqual(r.applied, ["T7", "T8", "R8"])
    }

    func testATaskThatNamesNoRefereeIsUnchanged() {
        let r = Rig()
        r.gate.offer(opener("U1"))
        r.gate.offer(move("U1", "R1", "confirm", "did:web:anyone.example", "pending"))
        r.gate.offer(move("U1", "R2", "expire", "did:web:anyone.example", "invalid"))
        r.gate.offer(move("U2", "C1", "claim", "did:plc:worker"))
        XCTAssertEqual(r.applied, ["U1", "R1", "R2", "C1"])
    }
}
