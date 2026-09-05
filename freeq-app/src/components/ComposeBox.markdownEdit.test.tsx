// @vitest-environment jsdom
// Editing must win over markdown mode: with both on, the send is an edit,
// not a second message.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent } from '@testing-library/react';

vi.mock('../irc/client', () => ({
  sendMessage: vi.fn(), sendReply: vi.fn(), sendEdit: vi.fn(), sendMarkdown: vi.fn(),
  sendAction: vi.fn(), joinChannel: vi.fn(), partChannel: vi.fn(), setTopic: vi.fn(),
  setMode: vi.fn(), kickUser: vi.fn(), inviteUser: vi.fn(), setAway: vi.fn(),
  rawCommand: vi.fn(), sendWhois: vi.fn(), startTyping: vi.fn(), stopTyping: vi.fn(),
  getClient: () => null,
}));

const { ComposeBox } = await import('./ComposeBox');
const { useStore } = await import('../store');
const client = await import('../irc/client');
const s = () => useStore.getState();

beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  s().reset(); s().setNick('me'); s().addChannel('#room'); s().setActiveChannel('#room');
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe('editing while markdown mode is on', () => {
  it('sends an edit, not a new message', () => {
    s().setEditingMsg({ msgId: 'ORIG1', text: 'old text', channel: '#room' });
    const view = render(<ComposeBox />);
    fireEvent.click(view.getByTitle('Enable markdown mode'));
    const input = view.getByTestId('compose-input') as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: '**new** text\n\nsecond paragraph' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(client.sendEdit).toHaveBeenCalledWith('#room', 'ORIG1', '**new** text\n\nsecond paragraph', {
      tags: { '+freeq.at/mime': 'text/markdown' },
    });
    expect(client.sendMarkdown).not.toHaveBeenCalled();
    expect(s().editingMsg).toBeNull();
  });
});
