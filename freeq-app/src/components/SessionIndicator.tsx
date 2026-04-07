import { useEffect } from 'react';
import { useStore } from '../store';
import { joinAvSession, leaveAvSession, endAvSession, startAvSession, getNick } from '../irc/client';
import type { AvSession, AvParticipant } from '../store';

/** Shows active AV session status in the channel header.
 *  Polls the REST API to discover sessions (doesn't rely solely on TAGMSG). */
export function SessionIndicator({ channel }: { channel: string }) {
  const avSessions = useStore((s) => s.avSessions);
  const activeAvSession = useStore((s) => s.activeAvSession);
  const avAudioActive = useStore((s) => s.avAudioActive);
  const authDid = useStore((s) => s.authDid);
  const connectionState = useStore((s) => s.connectionState);

  const isConnected = connectionState === 'connected';

  // Poll REST API for active session on this channel (catches sessions started
  // before we joined the channel or missed TAGMSG broadcasts)
  useEffect(() => {
    if (!isConnected || !channel.startsWith('#')) return;

    let cancelled = false;

    async function poll() {
      try {
        const resp = await fetch(`/api/v1/channels/${encodeURIComponent(channel)}/sessions`);
        if (!resp.ok || cancelled) return;
        const data = await resp.json();
        if (cancelled) return;

        if (data.active) {
          const store = useStore.getState();
          const existing = store.avSessions.get(data.active.id);
          if (!existing) {
            // Session exists on server but not in our store — add it
            const participants = new Map<string, AvParticipant>();
            for (const p of data.active.participants || []) {
              participants.set(p.nick, {
                did: p.did || '',
                nick: p.nick,
                role: p.role || 'speaker',
                joinedAt: new Date(p.joined_at * 1000),
              });
            }
            const session: AvSession = {
              id: data.active.id,
              channel: data.active.channel,
              createdBy: data.active.created_by || '',
              createdByNick: data.active.created_by_nick || '',
              title: data.active.title || undefined,
              participants,
              state: 'active',
              startedAt: new Date(data.active.created_at * 1000),
              irohTicket: data.active.iroh_ticket || undefined,
            };
            store.updateAvSession(session);
          }
        }
      } catch {
        // Ignore fetch errors
      }
    }

    poll();
    const timer = setInterval(poll, 5000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [channel, isConnected]);

  // Find active session for this channel
  const session = [...avSessions.values()].find(
    (s) => s.channel?.toLowerCase() === channel.toLowerCase() && s.state === 'active'
  );

  if (!session) {
    if (!authDid) return null;
    return (
      <button
        onClick={() => startAvSession(channel)}
        disabled={!isConnected}
        className={`text-xs px-2 py-1 rounded-lg flex items-center gap-1 ${
          isConnected
            ? 'text-fg-dim hover:text-accent hover:bg-bg-tertiary'
            : 'text-fg-dim/40 cursor-not-allowed'
        }`}
        title={isConnected ? "Start a voice session" : "Not connected"}
      >
        <PhoneIcon />
      </button>
    );
  }

  const isInSession = activeAvSession === session.id;
  const participantCount = session.participants.size;
  const myNick = getNick();
  const isHost = session.createdByNick.toLowerCase() === myNick.toLowerCase();

  const handleJoinWithAudio = () => {
    joinAvSession(channel, session.id);
    useStore.getState().setAvAudioActive(true);
  };

  const handleLeave = () => {
    useStore.getState().setAvAudioActive(false);
    leaveAvSession(channel, session.id);
  };

  const handleEnd = () => {
    useStore.getState().setAvAudioActive(false);
    endAvSession(channel, session.id);
  };

  const handleConnectAudio = () => {
    useStore.getState().setAvAudioActive(true);
  };

  return (
    <div className="flex items-center gap-2">
      <div className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium ${
        isInSession ? 'bg-success/15 text-success' : 'bg-accent/10 text-accent'
      }`}>
        <span className="w-2 h-2 rounded-full bg-current animate-pulse" />
        <span>{session.title || 'Voice'}</span>
        <span className="opacity-60">({participantCount})</span>
      </div>

      {!isInSession && (
        <button
          onClick={handleJoinWithAudio}
          className="text-xs px-2.5 py-1 rounded-lg bg-accent text-white hover:bg-accent/90 font-medium"
        >
          Join
        </button>
      )}

      {isInSession && (
        <div className="flex items-center gap-1">
          {!avAudioActive && (
            <button
              onClick={handleConnectAudio}
              className="text-xs px-2 py-1 rounded-lg bg-success/15 text-success hover:bg-success/25 font-medium flex items-center gap-1"
              title="Connect audio"
            >
              <MicIcon /> Audio
            </button>
          )}
          <button
            onClick={handleLeave}
            className="text-xs px-2 py-1 rounded-lg bg-danger/10 text-danger hover:bg-danger/20 font-medium"
          >
            Leave
          </button>
          {isHost && (
            <button
              onClick={handleEnd}
              className="text-xs px-2 py-1 rounded-lg text-danger hover:bg-danger/10"
              title="End session for everyone"
            >
              End
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function PhoneIcon() {
  return (
    <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="currentColor">
      <path d="M3.654 1.328a.678.678 0 0 0-1.015-.063L1.605 2.3c-.483.484-.661 1.169-.45 1.77a17.568 17.568 0 0 0 4.168 6.608 17.569 17.569 0 0 0 6.608 4.168c.601.211 1.286.033 1.77-.45l1.034-1.034a.678.678 0 0 0-.063-1.015l-2.307-1.794a.678.678 0 0 0-.58-.122l-2.19.547a1.745 1.745 0 0 1-1.657-.459L5.482 8.062a1.745 1.745 0 0 1-.46-1.657l.548-2.19a.678.678 0 0 0-.122-.58L3.654 1.328zM1.884.511a1.745 1.745 0 0 1 2.612.163L6.29 2.98c.329.423.445.974.315 1.494l-.547 2.19a.678.678 0 0 0 .178.643l2.457 2.457a.678.678 0 0 0 .644.178l2.189-.547a1.745 1.745 0 0 1 1.494.315l2.306 1.794c.829.645.905 1.87.163 2.611l-1.034 1.034c-.74.74-1.846 1.065-2.877.702a18.634 18.634 0 0 1-7.01-4.42 18.634 18.634 0 0 1-4.42-7.009c-.362-1.03-.037-2.137.703-2.877L1.885.511z"/>
    </svg>
  );
}

function MicIcon() {
  return (
    <svg className="w-3 h-3" viewBox="0 0 16 16" fill="currentColor">
      <path d="M3.5 6.5A.5.5 0 0 1 4 7v1a4 4 0 0 0 8 0V7a.5.5 0 0 1 1 0v1a5 5 0 0 1-4.5 4.975V15h3a.5.5 0 0 1 0 1h-7a.5.5 0 0 1 0-1h3v-2.025A5 5 0 0 1 3 8V7a.5.5 0 0 1 .5-.5z"/>
      <path d="M10 8a2 2 0 1 1-4 0V3a2 2 0 1 1 4 0v5zM8 0a3 3 0 0 0-3 3v5a3 3 0 0 0 6 0V3a3 3 0 0 0-3-3z"/>
    </svg>
  );
}
