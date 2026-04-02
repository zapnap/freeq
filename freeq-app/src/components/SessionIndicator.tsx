import { useState } from 'react';
import { useStore } from '../store';
import { joinAvSession, leaveAvSession, endAvSession, startAvSession, startSessionAudio, stopSessionAudio, getNick } from '../irc/client';

/** Shows active AV session status in the channel header. */
export function SessionIndicator({ channel }: { channel: string }) {
  const avSessions = useStore((s) => s.avSessions);
  const activeAvSession = useStore((s) => s.activeAvSession);
  const authDid = useStore((s) => s.authDid);
  const [audioActive, setAudioActive] = useState(false);
  const [muted, setMuted] = useState(false);

  // Find active session for this channel
  const session = [...avSessions.values()].find(
    (s) => s.channel?.toLowerCase() === channel.toLowerCase() && s.state === 'active'
  );

  if (!session) {
    if (!authDid) return null;
    return (
      <button
        onClick={() => startAvSession(channel)}
        className="text-fg-dim hover:text-accent text-xs px-2 py-1 rounded-lg hover:bg-bg-tertiary flex items-center gap-1"
        title="Start a voice session"
      >
        <PhoneIcon />
      </button>
    );
  }

  const isInSession = activeAvSession === session.id;
  const participantCount = session.participants.size;
  const myNick = getNick();
  const isHost = session.createdByNick.toLowerCase() === myNick.toLowerCase();

  const handleJoinWithAudio = async () => {
    joinAvSession(channel, session.id);
    // Small delay for the server to process the join before starting audio
    setTimeout(async () => {
      try {
        await startSessionAudio(channel);
        setAudioActive(true);
      } catch (e: any) {
        useStore.getState().addSystemMessage(channel, `Audio failed: ${e.message}`);
      }
    }, 500);
  };

  const handleStartAudio = async () => {
    try {
      await startSessionAudio(channel);
      setAudioActive(true);
    } catch (e: any) {
      useStore.getState().addSystemMessage(channel, `Audio failed: ${e.message}`);
    }
  };

  const handleToggleMute = async () => {
    const { toggleMute } = await import('../lib/webrtc-audio');
    const nowMuted = toggleMute();
    setMuted(nowMuted);
  };

  const handleLeave = () => {
    stopSessionAudio();
    setAudioActive(false);
    setMuted(false);
    leaveAvSession(channel, session.id);
  };

  const handleEnd = () => {
    stopSessionAudio();
    setAudioActive(false);
    setMuted(false);
    endAvSession(channel, session.id);
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
          {!audioActive ? (
            <button
              onClick={handleStartAudio}
              className="text-xs px-2 py-1 rounded-lg bg-success/15 text-success hover:bg-success/25 font-medium flex items-center gap-1"
              title="Start audio"
            >
              <MicIcon /> Audio
            </button>
          ) : (
            <button
              onClick={handleToggleMute}
              className={`text-xs px-2 py-1 rounded-lg font-medium flex items-center gap-1 ${
                muted ? 'bg-warning/15 text-warning' : 'bg-success/15 text-success'
              }`}
              title={muted ? 'Unmute' : 'Mute'}
            >
              {muted ? <MicOffIcon /> : <MicIcon />}
              {muted ? 'Muted' : 'Live'}
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

function MicOffIcon() {
  return (
    <svg className="w-3 h-3" viewBox="0 0 16 16" fill="currentColor">
      <path d="M13 8c0 .564-.094 1.107-.266 1.613l-.814-.814A4.02 4.02 0 0 0 12 8V7a.5.5 0 0 1 1 0v1zm-5 4c.818 0 1.578-.245 2.212-.667l.718.719a4.973 4.973 0 0 1-2.43.923V15h3a.5.5 0 0 1 0 1h-7a.5.5 0 0 1 0-1h3v-2.025A5 5 0 0 1 3 8V7a.5.5 0 0 1 1 0v1a4 4 0 0 0 4 4zm3-9v4.879L5.158 2.037A3.001 3.001 0 0 1 11 3z"/>
      <path d="M9.486 10.607 5 6.12V8a3 3 0 0 0 4.486 2.607zm-7.84-1.96l1.054 1.054A4.97 4.97 0 0 1 2.5 8V7a.5.5 0 0 0-1 0v1c0 .07.002.14.006.208z"/>
      <path d="M.354 1.646a.5.5 0 0 1 .708 0l14 14a.5.5 0 0 1-.708.708l-14-14a.5.5 0 0 1 0-.708z"/>
    </svg>
  );
}
