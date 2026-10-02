import Foundation

// A ruling on a task — a receipt, an expiry, a closed review window — counts
// here only when the server the task's opener named as its referee
// (`act-home`) signed it, and its signature checks out. The SDK's verdict
// already holds a ruling to the keys the referee's own host lists, so this is
// the rest: who signed it, and waiting for the verdict without letting the
// task's later events overtake it. Twin of the web's `lib/act-referee.ts` and
// Android's `ActReferee.kt`.
//
// A task whose opener names no referee is handled exactly as before.

/// The verbs only a task's home signs (`spec/act-transitions.json`).
let rulingVerbs: Set<String> = ["confirm", "expire", "auto-accept"]

/// One task event as the gate reads it, with what to file it from.
/// `verdict` is the SDK's verdict state in lower case, or nil when nothing
/// checked it.
struct GatedAct<Payload> {
    var taskId: String
    var eventId: String
    var verb: String
    var did: String?
    var fields: [String: String]
    var verdict: String?
    var payload: Payload
}

/// What the task's opener said about its referee.
enum RefereeOf: Equatable {
    /// The opener is not held yet.
    case noOpener
    /// The opener named this DID, or nil when it named none.
    case named(String?)
}

enum RulingDecision: Equatable { case apply, drop, wait }

/// What to do with a ruling at the head of its task's queue.
func judgeRuling(referee: String?, did: String?, verdict: String?) -> RulingDecision {
    guard let referee else { return .apply }
    if did != referee { return .drop }
    switch verdict {
    case "pending": return .wait
    case "invalid", "retired": return .drop
    // `device` and `server` count; `unverifiable`, `unsigned` and no verdict
    // at all (no checker, or a referee that could not answer) as today.
    default: return .apply
    }
}

/// Holds rulings until they can be judged, in the order their task's events
/// arrived: once a ruling waits, every later event of its task waits behind
/// it, so a ruling whose verdict settles late still lands in its place. A
/// ruling for a task whose opener has not arrived waits for the opener.
///
/// `schedule` runs its block once a pending verdict has waited long enough;
/// the ruling is then filed as one with no verdict.
final class RulingGate<Payload> {
    typealias Event = GatedAct<Payload>

    private let refereeOf: (String) -> RefereeOf
    private let apply: (Event) -> Void
    private let schedule: (@escaping () -> Void) -> Void
    private var queues: [String: [Event]] = [:]
    private var awaitingOpener: [String: [Event]] = [:]
    private var awaitingOrder: [String] = []
    private var timed: Set<String> = []

    /// How many tasks may hold rulings waiting for their opener at once.
    private static var maxWaitingTasks: Int { 256 }

    init(
        refereeOf: @escaping (String) -> RefereeOf,
        apply: @escaping (Event) -> Void,
        schedule: @escaping (@escaping () -> Void) -> Void
    ) {
        self.refereeOf = refereeOf
        self.apply = apply
        self.schedule = schedule
    }

    /// One task event, as it arrives.
    func offer(_ ev: Event) {
        if queues[ev.taskId] != nil {
            queues[ev.taskId]!.append(ev)
            return
        }
        if ev.eventId == ev.taskId {
            apply(ev)
            if let waiting = awaitingOpener.removeValue(forKey: ev.taskId) {
                awaitingOrder.removeAll { $0 == ev.taskId }
                queues[ev.taskId] = waiting
                drain(ev.taskId)
            }
            return
        }
        guard rulingVerbs.contains(ev.verb) else {
            apply(ev)
            return
        }
        if refereeOf(ev.taskId) == .noOpener {
            if awaitingOpener[ev.taskId] == nil { awaitingOrder.append(ev.taskId) }
            awaitingOpener[ev.taskId, default: []].append(ev)
            if awaitingOrder.count > Self.maxWaitingTasks {
                awaitingOpener.removeValue(forKey: awaitingOrder.removeFirst())
            }
            return
        }
        queues[ev.taskId] = [ev]
        drain(ev.taskId)
    }

    /// A verdict that was pending when its event arrived, now known.
    func settle(eventId: String, verdict: String) {
        guard let taskId = queues.first(where: { $0.value.first?.eventId == eventId })?.key
        else { return }
        queues[taskId]![0].verdict = verdict
        drain(taskId)
    }

    private func drain(_ taskId: String) {
        while let head = queues[taskId]?.first {
            if rulingVerbs.contains(head.verb) {
                let referee: String?
                if case .named(let did) = refereeOf(taskId) { referee = did } else { referee = nil }
                switch judgeRuling(referee: referee, did: head.did, verdict: head.verdict) {
                case .wait:
                    if timed.insert(head.eventId).inserted {
                        schedule { [weak self] in
                            guard let self else { return }
                            self.timed.remove(head.eventId)
                            guard self.queues[taskId]?.first?.eventId == head.eventId,
                                  self.queues[taskId]?.first?.verdict == "pending"
                            else { return }
                            self.queues[taskId]![0].verdict = nil
                            self.drain(taskId)
                        }
                    }
                    return
                case .drop:
                    queues[taskId]!.removeFirst()
                case .apply:
                    queues[taskId]!.removeFirst()
                    apply(head)
                }
            } else {
                queues[taskId]!.removeFirst()
                apply(head)
            }
        }
        queues.removeValue(forKey: taskId)
    }
}
