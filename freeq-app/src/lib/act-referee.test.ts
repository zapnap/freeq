import { afterEach, describe, expect, it, vi } from 'vitest';
import { RulingGate, type GatedEvent } from './act-referee';

const HOME = 'did:web:referee.example';

/** A gate over a log of applied event ids, with each task's opener as held. */
function gate() {
  const applied: string[] = [];
  const openers = new Map<string, Record<string, string>>();
  const g = new RulingGate<GatedEvent>(
    {
      refereeOf: (taskId) => {
        const fields = openers.get(taskId);
        return fields === undefined ? null : fields['act-home'];
      },
      apply: (ev) => {
        applied.push(ev.eventId);
        if (ev.eventId === ev.taskId) openers.set(ev.taskId, ev.fields);
      },
    },
    1_000,
  );
  return { g, applied };
}

function opener(taskId: string, home?: string): GatedEvent {
  return { taskId, eventId: taskId, verb: 'offer', did: 'did:plc:poster', fields: home ? { 'act-home': home } : {} };
}

function move(taskId: string, eventId: string, verb: string, did: string, state?: string): GatedEvent {
  return { taskId, eventId, verb, did, fields: {}, ...(state ? { verdict: { state } } : {}) };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('a ruling on a task that names its referee', () => {
  it('waits for its verdict, then lands in its place among the task’s events', () => {
    const { g, applied } = gate();
    g.offer(opener('T1', HOME));
    g.offer(move('T1', 'R1', 'confirm', HOME, 'pending'));
    g.offer(move('T1', 'P1', 'progress', 'did:plc:worker'));
    expect(applied).toEqual(['T1']);
    g.settle('R1', { state: 'device' });
    expect(applied).toEqual(['T1', 'R1', 'P1']);
  });

  it('counts on the referee’s device or server signature', () => {
    for (const state of ['device', 'server']) {
      const { g, applied } = gate();
      g.offer(opener('T2', HOME));
      g.offer(move('T2', 'R2', 'expire', HOME, state));
      expect(applied, state).toEqual(['T2', 'R2']);
    }
  });

  it('changes nothing when it fails the check, and the task carries on', () => {
    for (const state of ['invalid', 'retired']) {
      const { g, applied } = gate();
      g.offer(opener('T3', HOME));
      g.offer(move('T3', 'R3', 'expire', HOME, 'pending'));
      g.offer(move('T3', 'P3', 'progress', 'did:plc:worker'));
      g.settle('R3', { state });
      expect(applied, state).toEqual(['T3', 'P3']);
    }
  });

  it('changes nothing when anyone but the named referee signed it', () => {
    const { g, applied } = gate();
    g.offer(opener('T4', HOME));
    g.offer(move('T4', 'R4', 'confirm', 'did:web:elsewhere.example', 'device'));
    expect(applied).toEqual(['T4']);
  });

  it('applies as today on an unverifiable verdict, with no checker, or when the referee cannot answer', () => {
    for (const state of ['unverifiable', 'unsigned', undefined]) {
      const { g, applied } = gate();
      g.offer(opener('T5', HOME));
      g.offer(move('T5', 'R5', 'auto-accept', HOME, state));
      expect(applied, String(state)).toEqual(['T5', 'R5']);
    }
  });

  it('applies as today when its verdict never settles', () => {
    vi.useFakeTimers();
    const { g, applied } = gate();
    g.offer(opener('T6', HOME));
    g.offer(move('T6', 'R6', 'expire', HOME, 'pending'));
    vi.advanceTimersByTime(1_000);
    expect(applied).toEqual(['T6', 'R6']);
  });

  it('waits for its task’s opener when it arrives first, then is judged', () => {
    const { g, applied } = gate();
    g.offer(move('T7', 'R7', 'expire', 'did:web:elsewhere.example', 'device'));
    g.offer(move('T8', 'R8', 'expire', HOME, 'device'));
    expect(applied).toEqual([]);
    g.offer(opener('T7', HOME));
    g.offer(opener('T8', HOME));
    expect(applied).toEqual(['T7', 'T8', 'R8']);
  });
});

describe('a task that names no referee', () => {
  it('applies every ruling at once, whatever its verdict, as today', () => {
    const { g, applied } = gate();
    g.offer(opener('U1'));
    g.offer(move('U1', 'R1', 'confirm', 'did:web:anyone.example', 'pending'));
    g.offer(move('U1', 'R2', 'expire', 'did:web:anyone.example', 'invalid'));
    expect(applied).toEqual(['U1', 'R1', 'R2']);
  });

  it('applies every other move at once', () => {
    const { g, applied } = gate();
    g.offer(move('U2', 'C1', 'claim', 'did:plc:worker'));
    expect(applied).toEqual(['C1']);
  });
});
