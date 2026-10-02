import { RULING_VERBS } from '@freeq/sdk';

/**
 * A ruling on a task — a receipt, an expiry, a closed review window — counts
 * here only when the server the task's opener named as its referee
 * (`act-home`) signed it, and its signature checks out. The SDK's verdict
 * already holds a ruling to the keys the referee's own host lists, so this
 * is the rest: who signed it, and waiting for the verdict without letting the
 * task's later events overtake it.
 *
 * A task whose opener names no referee is handled exactly as before.
 */

/** What the gate reads of one task event. */
export interface GatedEvent {
  taskId: string;
  eventId: string;
  verb: string;
  did?: string;
  fields: Record<string, string>;
  verdict?: { state: string };
}

export interface GateHooks<E extends GatedEvent> {
  /** The referee the task's opener named: its DID, `undefined` when the
   *  opener named none, `null` when the opener is not held. */
  refereeOf(taskId: string): string | undefined | null;
  /** File the event, as every task event was filed before this gate. */
  apply(ev: E): void;
}

/** How many tasks may hold rulings waiting for their opener at once. */
const MAX_WAITING_TASKS = 256;

/** What to do with a ruling at the head of its task's queue. */
export function judgeRuling(referee: string | undefined | null, ev: GatedEvent): 'apply' | 'drop' | 'wait' {
  if (!referee) return 'apply';
  if (ev.did !== referee) return 'drop';
  switch (ev.verdict?.state) {
    case 'pending':
      return 'wait';
    case 'invalid':
    case 'retired':
      return 'drop';
    // `device` and `server` count; `unverifiable`, `unsigned` and no verdict
    // at all (no checker, or a referee that could not answer) as today.
    default:
      return 'apply';
  }
}

/**
 * Holds rulings until they can be judged, in the order their task's events
 * arrived: once a ruling waits, every later event of its task waits behind
 * it, so a ruling whose verdict settles late still lands in its place. A
 * ruling for a task whose opener has not arrived waits for the opener.
 */
export class RulingGate<E extends GatedEvent> {
  private readonly queues = new Map<string, E[]>();
  private readonly awaitingOpener = new Map<string, E[]>();
  private readonly timers = new Map<string, ReturnType<typeof setTimeout>>();
  private readonly hooks: GateHooks<E>;
  /** How long a verdict may stay pending before the ruling is filed as one
   *  with no verdict. */
  private readonly settleMs: number;

  constructor(hooks: GateHooks<E>, settleMs = 30_000) {
    this.hooks = hooks;
    this.settleMs = settleMs;
  }

  /** One task event, as it arrives. */
  offer(ev: E): void {
    const queue = this.queues.get(ev.taskId);
    if (queue) {
      queue.push(ev);
      return;
    }
    if (ev.eventId === ev.taskId) {
      this.hooks.apply(ev);
      const waiting = this.awaitingOpener.get(ev.taskId);
      if (waiting) {
        this.awaitingOpener.delete(ev.taskId);
        this.queues.set(ev.taskId, waiting);
        this.drain(ev.taskId);
      }
      return;
    }
    if (!RULING_VERBS.has(ev.verb)) {
      this.hooks.apply(ev);
      return;
    }
    if (this.hooks.refereeOf(ev.taskId) === null) {
      const waiting = this.awaitingOpener.get(ev.taskId) ?? [];
      waiting.push(ev);
      this.awaitingOpener.set(ev.taskId, waiting);
      if (this.awaitingOpener.size > MAX_WAITING_TASKS) {
        this.awaitingOpener.delete(this.awaitingOpener.keys().next().value!);
      }
      return;
    }
    this.queues.set(ev.taskId, [ev]);
    this.drain(ev.taskId);
  }

  /** A verdict that was pending when its event arrived, now known. */
  settle(eventId: string, verdict: { state: string }): void {
    for (const [taskId, queue] of this.queues) {
      if (queue[0]?.eventId !== eventId) continue;
      queue[0].verdict = verdict;
      this.drain(taskId);
      return;
    }
  }

  private drain(taskId: string): void {
    const queue = this.queues.get(taskId);
    if (!queue) return;
    const timer = this.timers.get(taskId);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.timers.delete(taskId);
    }
    while (queue.length > 0) {
      const head = queue[0]!;
      if (RULING_VERBS.has(head.verb)) {
        const decision = judgeRuling(this.hooks.refereeOf(taskId), head);
        if (decision === 'wait') {
          this.timers.set(
            taskId,
            setTimeout(() => {
              this.timers.delete(taskId);
              delete head.verdict;
              this.drain(taskId);
            }, this.settleMs),
          );
          return;
        }
        queue.shift();
        if (decision === 'apply') this.hooks.apply(head);
      } else {
        queue.shift();
        this.hooks.apply(head);
      }
    }
    this.queues.delete(taskId);
  }
}
