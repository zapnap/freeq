package com.freeq.model

/**
 * A ruling on a task — a receipt, an expiry, a closed review window — counts
 * here only when the server the task's opener named as its referee
 * (`act-home`) signed it, and its signature checks out. The SDK's verdict
 * already holds a ruling to the keys the referee's own host lists, so this
 * is the rest: who signed it, and waiting for the verdict without letting the
 * task's later events overtake it. Twin of the web's `lib/act-referee.ts`.
 *
 * A task whose opener names no referee is handled exactly as before.
 */

/** The verbs only a task's home signs (`spec/act-transitions.json`). */
val RULING_VERBS = setOf("confirm", "expire", "auto-accept")

/** What the gate reads of one task event. `verdict` is the SDK's verdict
 *  state in lower case, or null when nothing checked it. */
interface GatedEvent {
    val taskId: String
    val eventId: String
    val verb: String
    val did: String?
    val fields: Map<String, String>
    var verdict: String?
}

/** One task event off the SDK, with what to file it from. */
data class GatedAct(
    override val taskId: String,
    override val eventId: String,
    override val verb: String,
    override val did: String?,
    override val fields: Map<String, String>,
    override var verdict: String? = null,
    val event: com.freeq.ffi.ActEvent? = null,
) : GatedEvent

/** What the task's opener said about its referee. */
sealed class RefereeOf {
    /** The opener is not held yet. */
    object NoOpener : RefereeOf()
    /** The opener named `did`, or null when it named none. */
    data class Named(val did: String?) : RefereeOf()
}

enum class RulingDecision { APPLY, DROP, WAIT }

/** What to do with a ruling at the head of its task's queue. */
fun judgeRuling(referee: String?, ev: GatedEvent): RulingDecision {
    if (referee == null) return RulingDecision.APPLY
    if (ev.did != referee) return RulingDecision.DROP
    return when (ev.verdict) {
        "pending" -> RulingDecision.WAIT
        "invalid", "retired" -> RulingDecision.DROP
        // `device` and `server` count; `unverifiable`, `unsigned` and no
        // verdict at all (no checker, or a referee that could not answer) as
        // today.
        else -> RulingDecision.APPLY
    }
}

/**
 * Holds rulings until they can be judged, in the order their task's events
 * arrived: once a ruling waits, every later event of its task waits behind
 * it, so a ruling whose verdict settles late still lands in its place. A
 * ruling for a task whose opener has not arrived waits for the opener.
 *
 * `schedule` runs its block once a pending verdict has waited long enough;
 * the ruling is then filed as one with no verdict.
 */
class RulingGate<E : GatedEvent>(
    private val refereeOf: (String) -> RefereeOf,
    private val apply: (E) -> Unit,
    private val schedule: (() -> Unit) -> Unit,
) {
    private val queues = LinkedHashMap<String, ArrayDeque<E>>()
    private val awaitingOpener = LinkedHashMap<String, MutableList<E>>()
    private val timed = mutableSetOf<String>()

    /** One task event, as it arrives. */
    fun offer(ev: E) {
        queues[ev.taskId]?.let { it.addLast(ev); return }
        if (ev.eventId == ev.taskId) {
            apply(ev)
            awaitingOpener.remove(ev.taskId)?.let { waiting ->
                queues[ev.taskId] = ArrayDeque(waiting)
                drain(ev.taskId)
            }
            return
        }
        if (ev.verb !in RULING_VERBS) {
            apply(ev)
            return
        }
        if (refereeOf(ev.taskId) == RefereeOf.NoOpener) {
            awaitingOpener.getOrPut(ev.taskId) { mutableListOf() }.add(ev)
            if (awaitingOpener.size > MAX_WAITING_TASKS) {
                awaitingOpener.remove(awaitingOpener.keys.first())
            }
            return
        }
        queues[ev.taskId] = ArrayDeque(listOf(ev))
        drain(ev.taskId)
    }

    /** A verdict that was pending when its event arrived, now known. */
    fun settle(eventId: String, verdict: String) {
        for ((taskId, queue) in queues) {
            if (queue.firstOrNull()?.eventId != eventId) continue
            queue.first().verdict = verdict
            drain(taskId)
            return
        }
    }

    private fun drain(taskId: String) {
        val queue = queues[taskId] ?: return
        while (queue.isNotEmpty()) {
            val head = queue.first()
            if (head.verb in RULING_VERBS) {
                val referee = (refereeOf(taskId) as? RefereeOf.Named)?.did
                when (judgeRuling(referee, head)) {
                    RulingDecision.WAIT -> {
                        if (timed.add(head.eventId)) {
                            schedule {
                                timed.remove(head.eventId)
                                if (queues[taskId]?.firstOrNull() === head && head.verdict == "pending") {
                                    head.verdict = null
                                    drain(taskId)
                                }
                            }
                        }
                        return
                    }
                    RulingDecision.DROP -> queue.removeFirst()
                    RulingDecision.APPLY -> { queue.removeFirst(); apply(head) }
                }
            } else {
                queue.removeFirst()
                apply(head)
            }
        }
        queues.remove(taskId)
    }

    private companion object {
        /** How many tasks may hold rulings waiting for their opener at once. */
        const val MAX_WAITING_TASKS = 256
    }
}
