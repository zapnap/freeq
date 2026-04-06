import { useEffect, useRef, useCallback } from 'react';
import { useStore } from '../store';
import { getNick, leaveAvSession } from '../irc/client';
import { loadMoqComponents } from '../lib/moq-loader';

/**
 * Inline call panel — replaces the call.html popup.
 *
 * Renders as a compact bar when avAudioActive is true. Manages moq-publish
 * (microphone → MoQ SFU) and moq-watch (other participants' audio) elements
 * imperatively via refs, since they're custom web components.
 *
 * Handles retry on RESET_STREAM: failed watchers are removed and recreated
 * on the next participant poll cycle.
 */
export function CallPanel() {
  const activeAvSession = useStore((s) => s.activeAvSession);
  const avAudioActive = useStore((s) => s.avAudioActive);
  const avMuted = useStore((s) => s.avMuted);
  const avSessions = useStore((s) => s.avSessions);

  const session = activeAvSession ? avSessions.get(activeAvSession) : null;
  const sessionId = session?.id;
  const channel = session?.channel;

  const publishContainerRef = useRef<HTMLDivElement>(null);
  const watchContainerRef = useRef<HTMLDivElement>(null);
  const publishElRef = useRef<HTMLElement | null>(null);
  const watchersRef = useRef<Map<string, HTMLElement>>(new Map());
  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const retryCountRef = useRef<Map<string, number>>(new Map());

  const myNick = getNick();

  const moqOrigin = `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/av/moq`;

  // ── Start/stop audio when avAudioActive changes ─────────────
  useEffect(() => {
    if (!avAudioActive || !sessionId || !myNick) return;

    let cancelled = false;

    async function start() {
      try {
        await loadMoqComponents();
      } catch (e) {
        console.error('[call] Failed to load MoQ components:', e);
        useStore.getState().addSystemMessage(channel || 'server', 'Failed to load audio components');
        useStore.getState().setAvAudioActive(false);
        return;
      }

      if (cancelled) return;

      // Request microphone
      try {
        await navigator.mediaDevices.getUserMedia({ audio: true });
      } catch (e: unknown) {
        const err = e as { name?: string; message?: string };
        const reason = err.name === 'NotAllowedError'
          ? 'microphone permission denied'
          : err.name === 'NotFoundError'
          ? 'no microphone found'
          : err.message || 'unknown error';
        console.error('[call] Mic error:', reason);
        useStore.getState().addSystemMessage(channel || 'server', `Microphone error: ${reason}`);
        useStore.getState().setAvAudioActive(false);
        return;
      }

      if (cancelled) return;

      // Create publisher
      const container = publishContainerRef.current;
      if (!container) return;

      const pub = document.createElement('moq-publish');
      const broadcastName = `${sessionId}/${myNick}`;
      pub.setAttribute('url', moqOrigin);
      pub.setAttribute('name', broadcastName);
      pub.setAttribute('source', 'camera');
      container.appendChild(pub);
      publishElRef.current = pub;
      console.log('[call] Publishing:', broadcastName);

      // Start polling participants
      pollParticipants();
      pollTimerRef.current = setInterval(pollParticipants, 3000);
    }

    start();

    return () => {
      cancelled = true;
      cleanup();
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [avAudioActive, sessionId]);

  // ── Sync mute state to publisher ────────────────────────────
  useEffect(() => {
    const pub = publishElRef.current;
    if (pub) {
      (pub as HTMLElement & { muted?: boolean }).muted = avMuted;
    }
  }, [avMuted]);

  // ── Poll participants and manage watchers ────────────────────
  const pollParticipants = useCallback(async () => {
    if (!sessionId) return;

    try {
      const resp = await fetch(`/api/v1/sessions/${encodeURIComponent(sessionId)}`);
      if (!resp.ok) return;
      const data = await resp.json();
      if (!data.participants) return;

      const nicks: string[] = data.participants
        .map((p: { nick: string }) => p.nick)
        .filter((n: string) => n.toLowerCase() !== myNick.toLowerCase());

      const container = watchContainerRef.current;
      if (!container) return;

      // Add watchers for new participants
      for (const nick of nicks) {
        if (watchersRef.current.has(nick)) continue;

        // Check retry backoff
        const retries = retryCountRef.current.get(nick) || 0;
        if (retries > 0) {
          // Exponential backoff: skip this poll cycle if too recent
          // retries=1 → skip 1 cycle (3s), retries=2 → skip 2 (6s), max 3 skips (9s)
          const skipCycles = Math.min(retries, 3);
          retryCountRef.current.set(nick, retries - skipCycles); // count down
          if (retries > skipCycles) continue;
        }

        const broadcastName = `${sessionId}/${nick}`;
        console.log('[call] Subscribing to:', broadcastName);

        const watchEl = document.createElement('moq-watch');
        watchEl.setAttribute('jitter', '100');
        const canvas = document.createElement('canvas');
        canvas.style.display = 'none';
        watchEl.appendChild(canvas);
        container.appendChild(watchEl);
        watchEl.setAttribute('url', moqOrigin);
        watchEl.setAttribute('name', broadcastName);

        // Retry on error: remove element, will be recreated on next poll
        watchEl.addEventListener('error', () => {
          console.log('[call] Watch error for', nick, '— will retry');
          watchersRef.current.delete(nick);
          const count = retryCountRef.current.get(nick) || 0;
          retryCountRef.current.set(nick, Math.min(count + 1, 4));
          watchEl.setAttribute('url', '');
          watchEl.remove();
        });

        watchersRef.current.set(nick, watchEl);
        // Reset retry count on successful creation
        // (actual success is determined by not getting an error within a few seconds)
        setTimeout(() => {
          if (watchersRef.current.has(nick)) {
            retryCountRef.current.delete(nick);
          }
        }, 10000);
      }

      // Remove watchers for participants who left
      for (const [nick, el] of watchersRef.current) {
        if (!nicks.includes(nick)) {
          console.log('[call] Removing watcher for:', nick);
          el.setAttribute('url', '');
          el.remove();
          watchersRef.current.delete(nick);
          retryCountRef.current.delete(nick);
        }
      }
    } catch (e) {
      console.warn('[call] Poll failed:', e);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, myNick, moqOrigin]);

  // ── Cleanup ─────────────────────────────────────────────────
  function cleanup() {
    if (pollTimerRef.current) {
      clearInterval(pollTimerRef.current);
      pollTimerRef.current = null;
    }
    const pub = publishElRef.current;
    if (pub) {
      pub.setAttribute('source', '');
      pub.setAttribute('url', '');
      pub.remove();
      publishElRef.current = null;
    }
    for (const [, el] of watchersRef.current) {
      el.setAttribute('url', '');
      el.remove();
    }
    watchersRef.current.clear();
    retryCountRef.current.clear();
  }

  const handleMuteToggle = () => {
    useStore.getState().setAvMuted(!avMuted);
  };

  const handleLeave = () => {
    cleanup();
    useStore.getState().setAvAudioActive(false);
    if (channel && sessionId) {
      leaveAvSession(channel, sessionId);
    }
  };

  if (!avAudioActive || !sessionId) return null;

  const participantCount = session?.participants.size || 0;

  return (
    <div className="flex items-center gap-2 px-3 py-1.5 bg-success/10 border-b border-border text-sm">
      {/* Status */}
      <div className="flex items-center gap-1.5 text-success font-medium">
        <span className="w-2 h-2 rounded-full bg-success animate-pulse" />
        <span>Voice ({participantCount})</span>
      </div>

      <div className="flex-1" />

      {/* Mute */}
      <button
        onClick={handleMuteToggle}
        className={`text-xs px-2 py-1 rounded-lg font-medium ${
          avMuted
            ? 'bg-danger/15 text-danger'
            : 'bg-bg-tertiary text-fg-dim hover:text-fg'
        }`}
      >
        {avMuted ? <MicOffIcon /> : <MicIcon />}
        <span className="ml-1">{avMuted ? 'Unmute' : 'Mute'}</span>
      </button>

      {/* Leave */}
      <button
        onClick={handleLeave}
        className="text-xs px-2 py-1 rounded-lg bg-danger/10 text-danger hover:bg-danger/20 font-medium"
      >
        Leave
      </button>

      {/* Hidden containers for moq elements */}
      <div ref={publishContainerRef} className="hidden" />
      <div ref={watchContainerRef} className="hidden" />
    </div>
  );
}

function MicIcon() {
  return (
    <svg className="w-3 h-3 inline" viewBox="0 0 16 16" fill="currentColor">
      <path d="M3.5 6.5A.5.5 0 0 1 4 7v1a4 4 0 0 0 8 0V7a.5.5 0 0 1 1 0v1a5 5 0 0 1-4.5 4.975V15h3a.5.5 0 0 1 0 1h-7a.5.5 0 0 1 0-1h3v-2.025A5 5 0 0 1 3 8V7a.5.5 0 0 1 .5-.5z"/>
      <path d="M10 8a2 2 0 1 1-4 0V3a2 2 0 1 1 4 0v5zM8 0a3 3 0 0 0-3 3v5a3 3 0 0 0 6 0V3a3 3 0 0 0-3-3z"/>
    </svg>
  );
}

function MicOffIcon() {
  return (
    <svg className="w-3 h-3 inline" viewBox="0 0 16 16" fill="currentColor">
      <path d="M13 8c0 .564-.094 1.107-.266 1.613l-.814-.814A4.02 4.02 0 0 0 12 8V7a.5.5 0 0 1 1 0v1zm-5 4c.818 0 1.578-.245 2.212-.667l.718.719a4.973 4.973 0 0 1-2.43.923V15h3a.5.5 0 0 1 0 1h-7a.5.5 0 0 1 0-1h3v-2.025A5 5 0 0 1 3 8V7a.5.5 0 0 1 1 0v1a4 4 0 0 0 4 4zm3-9v4.879L5.158 2.037A3.001 3.001 0 0 1 11 3z"/>
      <path d="M9.486 10.607 5 6.12V8a3 3 0 0 0 4.486 2.607zm-7.84-1.96-.001-.001 1.442-1.442-.001-.001L14.96.33l.708.707L1.354 15.354l-.707-.707L4.14 11.153A4.985 4.985 0 0 1 3 8V7a.5.5 0 0 1 1 0v1c0 .455.076.897.216 1.306l.59-.59A4.02 4.02 0 0 1 4 8z"/>
    </svg>
  );
}
