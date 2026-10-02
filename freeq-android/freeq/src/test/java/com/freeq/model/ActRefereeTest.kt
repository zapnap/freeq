package com.freeq.model

import org.junit.Assert.assertEquals
import org.junit.Test

class ActRefereeTest {
    private val home = "did:web:referee.example"

    /** A gate over a log of applied ids, with each task's opener as held,
     *  and the timers it set, to fire by hand. */
    private class Rig {
        val applied = mutableListOf<String>()
        val openers = mutableMapOf<String, Map<String, String>>()
        val timers = mutableListOf<() -> Unit>()
        val gate = RulingGate<GatedAct>(
            refereeOf = { taskId -> if (taskId in openers) RefereeOf.Named(openers[taskId]!!["act-home"]) else RefereeOf.NoOpener },
            apply = { ev ->
                applied += ev.eventId
                if (ev.eventId == ev.taskId) openers[ev.taskId] = ev.fields
            },
            schedule = { run -> timers += run },
        )
    }

    private fun opener(taskId: String, home: String? = null) = GatedAct(
        taskId = taskId, eventId = taskId, verb = "offer", did = "did:plc:poster",
        fields = if (home != null) mapOf("act-home" to home) else emptyMap(),
    )

    private fun move(taskId: String, eventId: String, verb: String, did: String, state: String? = null) =
        GatedAct(taskId = taskId, eventId = eventId, verb = verb, did = did, fields = emptyMap(), verdict = state)

    @Test fun a_ruling_waits_for_its_verdict_and_lands_in_its_place() {
        val r = Rig()
        r.gate.offer(opener("T1", home))
        r.gate.offer(move("T1", "R1", "confirm", home, "pending"))
        r.gate.offer(move("T1", "P1", "progress", "did:plc:worker"))
        assertEquals(listOf("T1"), r.applied)
        r.gate.settle("R1", "device")
        assertEquals(listOf("T1", "R1", "P1"), r.applied)
    }

    @Test fun a_ruling_counts_on_the_referees_device_or_server_signature() {
        for (state in listOf("device", "server")) {
            val r = Rig()
            r.gate.offer(opener("T2", home))
            r.gate.offer(move("T2", "R2", "expire", home, state))
            assertEquals(state, listOf("T2", "R2"), r.applied)
        }
    }

    @Test fun a_ruling_that_fails_its_check_changes_nothing() {
        for (state in listOf("invalid", "retired")) {
            val r = Rig()
            r.gate.offer(opener("T3", home))
            r.gate.offer(move("T3", "R3", "expire", home, "pending"))
            r.gate.offer(move("T3", "P3", "progress", "did:plc:worker"))
            r.gate.settle("R3", state)
            assertEquals(state, listOf("T3", "P3"), r.applied)
        }
    }

    @Test fun a_ruling_another_server_signed_changes_nothing() {
        val r = Rig()
        r.gate.offer(opener("T4", home))
        r.gate.offer(move("T4", "R4", "confirm", "did:web:elsewhere.example", "device"))
        assertEquals(listOf("T4"), r.applied)
    }

    @Test fun a_ruling_with_no_checkable_verdict_applies_as_today() {
        for (state in listOf("unverifiable", "unsigned", null)) {
            val r = Rig()
            r.gate.offer(opener("T5", home))
            r.gate.offer(move("T5", "R5", "auto-accept", home, state))
            assertEquals(state.toString(), listOf("T5", "R5"), r.applied)
        }
    }

    @Test fun a_ruling_whose_verdict_never_settles_applies_as_today() {
        val r = Rig()
        r.gate.offer(opener("T6", home))
        r.gate.offer(move("T6", "R6", "expire", home, "pending"))
        assertEquals(listOf("T6"), r.applied)
        r.timers.single()()
        assertEquals(listOf("T6", "R6"), r.applied)
    }

    @Test fun a_ruling_waits_for_its_tasks_opener() {
        val r = Rig()
        r.gate.offer(move("T7", "R7", "expire", "did:web:elsewhere.example", "device"))
        r.gate.offer(move("T8", "R8", "expire", home, "device"))
        assertEquals(emptyList<String>(), r.applied)
        r.gate.offer(opener("T7", home))
        r.gate.offer(opener("T8", home))
        assertEquals(listOf("T7", "T8", "R8"), r.applied)
    }

    @Test fun a_task_that_names_no_referee_is_unchanged() {
        val r = Rig()
        r.gate.offer(opener("U1"))
        r.gate.offer(move("U1", "R1", "confirm", "did:web:anyone.example", "pending"))
        r.gate.offer(move("U1", "R2", "expire", "did:web:anyone.example", "invalid"))
        r.gate.offer(move("U2", "C1", "claim", "did:plc:worker"))
        assertEquals(listOf("U1", "R1", "R2", "C1"), r.applied)
    }
}
