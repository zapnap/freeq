using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Freeq.WinUI.Models;
using Freeq.WinUI.Services;
using Microsoft.UI.Dispatching;
using System.Collections.ObjectModel;

namespace Freeq.WinUI.ViewModels;

#pragma warning disable MVVMTK0045 // AOT compatibility - not needed for desktop WinUI

public partial class MainViewModel : ObservableObject
{
    private readonly IrcClient _irc = new();
    private readonly DispatcherQueue _dispatcher;

    [ObservableProperty] private string _serverUrl = "wss://irc.freeq.at/irc";
    [ObservableProperty] private string _nickname = "";
    [ObservableProperty] private string _connectionStatus = "Disconnected";
    [ObservableProperty] private ConnectionState _connectionState = ConnectionState.Disconnected;
    [ObservableProperty] private ChannelModel? _selectedChannel;
    [ObservableProperty] private string _composeText = "";
    [ObservableProperty] private string _currentTopic = "";
    [ObservableProperty] private bool _isConnected;
    [ObservableProperty] private bool _showConnectDialog = true;
    [ObservableProperty] private bool _isMemberListVisible = true;
    [ObservableProperty] private string? _errorMessage;

    // Edit mode: non-null when the user is editing an existing message
    [ObservableProperty] private string? _editingMessageId;
    [ObservableProperty] private bool _isEditing;

    // Typing indicator text shown above compose box, e.g. "Alice is typing…"
    [ObservableProperty] private string _typingText = "";

    public ObservableCollection<ChannelModel> Channels { get; } = [];
    public ObservableCollection<ChannelModel> FavoriteChannels { get; } = [];
    public ObservableCollection<ChannelModel> DirectMessages { get; } = [];
    public ObservableCollection<MessageModel> Messages { get; } = [];
    public ObservableCollection<MemberModel> Operators { get; } = [];
    public ObservableCollection<MemberModel> Voiced { get; } = [];
    public ObservableCollection<MemberModel> Members { get; } = [];

    private readonly Dictionary<string, List<MessageModel>> _messagesByChannel = new(StringComparer.OrdinalIgnoreCase);
    private readonly Dictionary<string, List<string>> _membersByChannel = new(StringComparer.OrdinalIgnoreCase);
    private readonly List<string> _pendingJoinChannels = [];
    private string? _desiredNickname;

    // Tracks who is currently typing per channel: channel → (nick → expiry timer)
    private readonly Dictionary<string, Dictionary<string, System.Threading.Timer>> _typingTimers = new(StringComparer.OrdinalIgnoreCase);

    // Outbound typing: timer to send "done" after 10 s of no input
    private System.Threading.Timer? _outboundTypingTimer;
    private bool _outboundTypingActive;

    public MainViewModel(DispatcherQueue dispatcher)
    {
        _dispatcher = dispatcher;

        _irc.StateChanged += OnStateChanged;
        _irc.MessageReceived += OnMessageReceived;
        _irc.MessageEdited += OnMessageEdited;
        _irc.MessageDeleted += OnMessageDeleted;
        _irc.TypingChanged += OnTypingChanged;
        _irc.JoinReceived += OnJoinReceived;
        _irc.PartReceived += OnPartReceived;
        _irc.QuitReceived += OnQuitReceived;
        _irc.TopicReceived += OnTopicReceived;
        _irc.NamesReceived += OnNamesReceived;
        _irc.NickChanged += OnNickChanged;
        _irc.AccountChanged += OnAccountChanged;
        _irc.ErrorReceived += OnError;
    }

    partial void OnSelectedChannelChanged(ChannelModel? value)
    {
        if (value == null) return;

        Messages.Clear();
        if (_messagesByChannel.TryGetValue(value.Name, out var msgs))
        {
            foreach (var m in msgs)
                Messages.Add(m);
        }

        CurrentTopic = value.Topic;
        RefreshMemberList(value.Name);

        value.UnreadCount = 0;
        value.MentionCount = 0;

        // Cancel any edit in progress when switching channels
        CancelEdit();
    }

    [RelayCommand]
    private async Task Connect()
    {
        if (string.IsNullOrWhiteSpace(Nickname))
        {
            ErrorMessage = "Please enter a nickname";
            return;
        }

        ErrorMessage = null;
        _desiredNickname = Nickname;
        await _irc.ConnectAsync(ServerUrl, Nickname);
    }

    [RelayCommand]
    private void Disconnect()
    {
        _irc.Disconnect();
        Channels.Clear();
        FavoriteChannels.Clear();
        DirectMessages.Clear();
        Messages.Clear();
        Operators.Clear();
        Voiced.Clear();
        Members.Clear();
        _messagesByChannel.Clear();
        _membersByChannel.Clear();
        _pendingJoinChannels.Clear();
        _typingTimers.Clear();
        SelectedChannel = null;
        ShowConnectDialog = true;
        IsConnected = false;
        CancelEdit();
    }

    [RelayCommand]
    private void SendMessage()
    {
        if (string.IsNullOrWhiteSpace(ComposeText) || SelectedChannel == null) return;

        var text = ComposeText.Trim();

        // Stop outbound typing indicator
        StopOutboundTyping();

        // Editing an existing message
        if (IsEditing && EditingMessageId != null)
        {
            _irc.SendEditMessage(SelectedChannel.Name, EditingMessageId, text);
            CancelEdit();
            return;
        }

        // Slash commands
        if (text.StartsWith('/'))
        {
            HandleSlashCommand(text);
            ComposeText = "";
            return;
        }

        // Regular message
        _irc.SendMessage(SelectedChannel.Name, text);

        // When echo-message is acked, the server echoes back with the real msgid.
        // When not acked, add a local message immediately.
        if (!_irc.IsEchoMessageAcked)
        {
            var msg = new MessageModel
            {
                Id = Guid.NewGuid().ToString(),
                Nick = _irc.Nick,
                Content = text,
                Timestamp = DateTimeOffset.Now,
            };
            AddMessage(SelectedChannel.Name, msg);
        }

        ComposeText = "";
    }

    /// <summary>Called by ComposeBox on every keystroke to send outbound typing indicators.</summary>
    public void NotifyTyping()
    {
        if (SelectedChannel == null) return;

        if (!_outboundTypingActive)
        {
            _outboundTypingActive = true;
            _irc.SendTyping(SelectedChannel.Name, true);
        }

        // Reset the 10-second done timer
        _outboundTypingTimer?.Dispose();
        _outboundTypingTimer = new System.Threading.Timer(_ =>
        {
            StopOutboundTyping();
        }, null, TimeSpan.FromSeconds(10), Timeout.InfiniteTimeSpan);
    }

    private void StopOutboundTyping()
    {
        _outboundTypingTimer?.Dispose();
        _outboundTypingTimer = null;
        if (_outboundTypingActive && SelectedChannel != null)
        {
            _outboundTypingActive = false;
            _irc.SendTyping(SelectedChannel.Name, false);
        }
    }

    /// <summary>Load a sent message into the compose box for editing.</summary>
    public void BeginEditMessage(MessageModel msg)
    {
        EditingMessageId = msg.Id;
        IsEditing = true;
        ComposeText = msg.Content;
    }

    [RelayCommand]
    public void CancelEdit()
    {
        EditingMessageId = null;
        IsEditing = false;
        ComposeText = "";
    }

    /// <summary>Send a soft-delete request for a message.</summary>
    public void DeleteMessage(MessageModel msg)
    {
        if (SelectedChannel == null) return;
        _irc.SendDeleteMessage(SelectedChannel.Name, msg.Id);
    }

    /// <summary>Returns all nicks in the current channel for Tab autocomplete.</summary>
    public IEnumerable<string> GetCurrentChannelNicks()
    {
        return Operators.Select(m => m.Nick)
            .Concat(Voiced.Select(m => m.Nick))
            .Concat(Members.Select(m => m.Nick));
    }

    /// <summary>Returns the last message sent by the current user in the active channel.</summary>
    public MessageModel? GetLastOwnMessage()
    {
        if (SelectedChannel == null) return null;
        if (!_messagesByChannel.TryGetValue(SelectedChannel.Name, out var msgs)) return null;
        return msgs.LastOrDefault(m =>
            m.Nick.Equals(_irc.Nick, StringComparison.OrdinalIgnoreCase) && !m.IsDeleted);
    }

    private void HandleSlashCommand(string text)
    {
        var space = text.IndexOf(' ');
        var cmd = (space < 0 ? text[1..] : text[1..space]).ToLowerInvariant();
        var args = space >= 0 ? text[(space + 1)..].Trim() : "";

        switch (cmd)
        {
            case "me":
                if (!string.IsNullOrWhiteSpace(args) && SelectedChannel != null)
                    _irc.Send($"PRIVMSG {SelectedChannel.Name} :\x01ACTION {args}\x01");
                break;

            case "join":
                if (!string.IsNullOrWhiteSpace(args))
                    JoinChannelCommand.Execute(args.Split(' ')[0]);
                break;

            case "part":
                if (SelectedChannel != null)
                    _irc.PartChannel(SelectedChannel.Name, string.IsNullOrWhiteSpace(args) ? null : args);
                break;

            case "topic":
                if (!string.IsNullOrWhiteSpace(args) && SelectedChannel != null)
                    _irc.Send($"TOPIC {SelectedChannel.Name} :{args}");
                break;

            case "invite":
            {
                var parts = args.Split(' ', 2);
                if (parts.Length >= 1 && SelectedChannel != null)
                {
                    var target = parts.Length >= 2 ? parts[1] : SelectedChannel.Name;
                    _irc.Send($"INVITE {parts[0]} {target}");
                }
                break;
            }

            case "kick":
            {
                var parts = args.Split(' ', 2);
                if (parts.Length >= 1 && SelectedChannel != null)
                {
                    var reason = parts.Length >= 2 ? parts[1] : "";
                    _irc.Send(string.IsNullOrEmpty(reason)
                        ? $"KICK {SelectedChannel.Name} {parts[0]}"
                        : $"KICK {SelectedChannel.Name} {parts[0]} :{reason}");
                }
                break;
            }

            case "op":
                if (!string.IsNullOrWhiteSpace(args) && SelectedChannel != null)
                    _irc.Send($"MODE {SelectedChannel.Name} +o {args}");
                break;

            case "deop":
                if (!string.IsNullOrWhiteSpace(args) && SelectedChannel != null)
                    _irc.Send($"MODE {SelectedChannel.Name} -o {args}");
                break;

            case "voice":
                if (!string.IsNullOrWhiteSpace(args) && SelectedChannel != null)
                    _irc.Send($"MODE {SelectedChannel.Name} +v {args}");
                break;

            case "mode":
                if (!string.IsNullOrWhiteSpace(args) && SelectedChannel != null)
                    _irc.Send($"MODE {SelectedChannel.Name} {args}");
                break;

            case "msg":
            {
                var parts = args.Split(' ', 2);
                if (parts.Length >= 2)
                    _irc.Send($"PRIVMSG {parts[0]} :{parts[1]}");
                break;
            }

            case "whois":
                if (!string.IsNullOrWhiteSpace(args))
                    _irc.Send($"WHOIS {args}");
                break;

            case "away":
                _irc.Send(string.IsNullOrWhiteSpace(args) ? "AWAY" : $"AWAY :{args}");
                break;

            case "pins":
                if (SelectedChannel != null)
                    _irc.RequestPins(SelectedChannel.Name);
                break;

            case "raw":
                if (!string.IsNullOrWhiteSpace(args))
                    _irc.Send(args);
                break;

            case "help":
                if (SelectedChannel != null)
                    AddSystemMessage(SelectedChannel.Name,
                        "Commands: /me /join /part /topic /invite /kick /op /deop /voice /mode /msg /whois /away /pins /raw /help");
                break;

            default:
                if (SelectedChannel != null)
                    AddSystemMessage(SelectedChannel.Name, $"Unknown command: /{cmd}");
                break;
        }
    }

    /// <summary>
    /// Queue channels to join. They will be sent to the server once registration completes (001).
    /// </summary>
    public void QueueJoinChannels(IEnumerable<string> channels)
    {
        foreach (var raw in channels)
        {
            var channel = raw.Trim();
            if (string.IsNullOrWhiteSpace(channel)) continue;
            if (!channel.StartsWith('#')) channel = "#" + channel;

            if (!_pendingJoinChannels.Contains(channel, StringComparer.OrdinalIgnoreCase))
                _pendingJoinChannels.Add(channel);
        }
    }

    [RelayCommand]
    private void JoinChannel(string channel)
    {
        if (string.IsNullOrWhiteSpace(channel)) return;
        if (!channel.StartsWith('#')) channel = "#" + channel;

        if (_irc.State == ConnectionState.Authenticated)
        {
            _irc.JoinChannel(channel);
        }
        else
        {
            if (!_pendingJoinChannels.Contains(channel, StringComparer.OrdinalIgnoreCase))
                _pendingJoinChannels.Add(channel);
        }
    }

    [RelayCommand]
    private void ToggleMemberList()
    {
        IsMemberListVisible = !IsMemberListVisible;
    }

    [RelayCommand]
    private void ToggleFavorite(ChannelModel? channel)
    {
        if (channel == null) return;
        channel.IsFavorite = !channel.IsFavorite;
        if (channel.IsFavorite)
        {
            if (!FavoriteChannels.Contains(channel))
                FavoriteChannels.Add(channel);
        }
        else
        {
            FavoriteChannels.Remove(channel);
        }
    }

    private void OnStateChanged(ConnectionState state)
    {
        _dispatcher.TryEnqueue(() =>
        {
            ConnectionState = state;
            ConnectionStatus = state switch
            {
                ConnectionState.Disconnected => "Disconnected",
                ConnectionState.Connecting => "Connecting...",
                ConnectionState.Connected => "Connected",
                ConnectionState.Authenticating => "Authenticating...",
                ConnectionState.Authenticated => "Authenticated",
                _ => "Unknown"
            };
            IsConnected = state is ConnectionState.Connected or ConnectionState.Authenticated;

            if (state == ConnectionState.Authenticated)
            {
                if (!string.IsNullOrEmpty(_desiredNickname) &&
                    !_irc.Nick.Equals(_desiredNickname, StringComparison.OrdinalIgnoreCase))
                {
                    _irc.Send($"NICK {_desiredNickname}");
                }

                Nickname = _irc.Nick;
                ShowConnectDialog = false;

                var toJoin = _pendingJoinChannels.ToList();
                _pendingJoinChannels.Clear();
                foreach (var ch in toJoin)
                    _irc.JoinChannel(ch);
            }
            else if (state == ConnectionState.Disconnected)
            {
                ShowConnectDialog = true;
                _pendingJoinChannels.Clear();
                _typingTimers.Clear();
                TypingText = "";
            }
        });
    }

    private void OnMessageReceived(string channel, string nick, string message, string? msgid, DateTimeOffset? serverTime)
    {
        _dispatcher.TryEnqueue(() =>
        {
            EnsureChannel(channel);

            var msg = new MessageModel
            {
                Id = msgid ?? Guid.NewGuid().ToString(),
                Nick = nick,
                Content = message,
                Timestamp = serverTime ?? DateTimeOffset.Now,
            };

            AddMessage(channel, msg);

            if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) != true)
            {
                var ch = FindChannel(channel);
                if (ch != null)
                {
                    ch.UnreadCount++;
                    if (message.Contains(_irc.Nick, StringComparison.OrdinalIgnoreCase))
                        ch.MentionCount++;
                }
            }
        });
    }

    private void OnMessageEdited(string channel, string originalMsgId, string newContent, DateTimeOffset? serverTime)
    {
        _dispatcher.TryEnqueue(() =>
        {
            if (!_messagesByChannel.TryGetValue(channel, out var msgs)) return;
            var msg = msgs.FirstOrDefault(m => m.Id == originalMsgId);
            if (msg != null)
            {
                msg.Content = newContent;
                msg.IsEdited = true;
            }
        });
    }

    private void OnMessageDeleted(string channel, string msgId)
    {
        _dispatcher.TryEnqueue(() =>
        {
            if (!_messagesByChannel.TryGetValue(channel, out var msgs)) return;
            var msg = msgs.FirstOrDefault(m => m.Id == msgId);
            if (msg != null)
                msg.IsDeleted = true;
        });
    }

    private void OnTypingChanged(string channel, string nick, bool isTyping)
    {
        // Ignore echoes of our own typing
        if (nick.Equals(_irc.Nick, StringComparison.OrdinalIgnoreCase)) return;

        _dispatcher.TryEnqueue(() =>
        {
            if (!_typingTimers.ContainsKey(channel))
                _typingTimers[channel] = new Dictionary<string, System.Threading.Timer>(StringComparer.OrdinalIgnoreCase);

            var channelTimers = _typingTimers[channel];

            if (isTyping)
            {
                if (channelTimers.TryGetValue(nick, out var existing))
                    existing.Dispose();

                channelTimers[nick] = new System.Threading.Timer(_ =>
                {
                    _dispatcher.TryEnqueue(() =>
                    {
                        RemoveTypingNick(channel, nick);
                        UpdateTypingText(channel);
                    });
                }, null, TimeSpan.FromSeconds(10), Timeout.InfiniteTimeSpan);
            }
            else
            {
                RemoveTypingNick(channel, nick);
            }

            UpdateTypingText(channel);
        });
    }

    private void RemoveTypingNick(string channel, string nick)
    {
        if (!_typingTimers.TryGetValue(channel, out var timers)) return;
        if (timers.TryGetValue(nick, out var t))
        {
            t.Dispose();
            timers.Remove(nick);
        }
    }

    private void UpdateTypingText(string channel)
    {
        if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) != true) return;

        if (!_typingTimers.TryGetValue(channel, out var timers) || timers.Count == 0)
        {
            TypingText = "";
            return;
        }

        var nicks = timers.Keys.ToList();
        TypingText = nicks.Count switch
        {
            1 => $"{nicks[0]} is typing…",
            2 => $"{nicks[0]} and {nicks[1]} are typing…",
            _ => "Several people are typing…"
        };
    }

    private void OnJoinReceived(string channel, string nick)
    {
        _dispatcher.TryEnqueue(() =>
        {
            EnsureChannel(channel);

            if (nick.Equals(_irc.Nick, StringComparison.OrdinalIgnoreCase))
            {
                var ch = FindChannel(channel);
                if (ch != null)
                    SelectedChannel = ch;

                AddMemberToChannel(channel, nick);
            }
            else
            {
                AddMemberToChannel(channel, nick);
            }

            AddSystemMessage(channel, $"{nick} has joined {channel}");
        });
    }

    private void OnPartReceived(string channel, string nick, string reason)
    {
        _dispatcher.TryEnqueue(() =>
        {
            if (nick.Equals(_irc.Nick, StringComparison.OrdinalIgnoreCase))
            {
                var ch = FindChannel(channel);
                if (ch != null)
                {
                    Channels.Remove(ch);
                    FavoriteChannels.Remove(ch);
                    _messagesByChannel.Remove(channel);
                    _membersByChannel.Remove(channel);
                    if (SelectedChannel == ch)
                        SelectedChannel = Channels.FirstOrDefault();
                }
            }
            else
            {
                RemoveMemberFromChannel(channel, nick);
                AddSystemMessage(channel, $"{nick} has left {channel}" + (string.IsNullOrEmpty(reason) ? "" : $" ({reason})"));
            }
        });
    }

    private void OnQuitReceived(string nick, string reason)
    {
        _dispatcher.TryEnqueue(() =>
        {
            foreach (var kvp in _membersByChannel)
            {
                kvp.Value.RemoveAll(n => n.TrimStart('@', '%', '+').Equals(nick, StringComparison.OrdinalIgnoreCase));
            }
            if (SelectedChannel != null)
                RefreshMemberList(SelectedChannel.Name);
        });
    }

    private void OnTopicReceived(string channel, string topic)
    {
        _dispatcher.TryEnqueue(() =>
        {
            var ch = FindChannel(channel);
            if (ch != null)
                ch.Topic = topic;
            if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) == true)
                CurrentTopic = topic;
        });
    }

    private void OnNamesReceived(string channel, List<string> nicks)
    {
        _dispatcher.TryEnqueue(() =>
        {
            if (!_membersByChannel.ContainsKey(channel))
                _membersByChannel[channel] = [];

            var list = _membersByChannel[channel];
            foreach (var nick in nicks)
            {
                var bare = nick.TrimStart('@', '%', '+');
                list.RemoveAll(n => n.TrimStart('@', '%', '+').Equals(bare, StringComparison.OrdinalIgnoreCase));
                list.Add(nick);
            }

            if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) == true)
                RefreshMemberList(channel);
        });
    }

    private void OnAccountChanged(string nick, string? account)
    {
        _dispatcher.TryEnqueue(() =>
        {
            if (SelectedChannel != null)
                RefreshMemberList(SelectedChannel.Name);
        });
    }

    private void OnNickChanged(string oldNick, string newNick)
    {
        _dispatcher.TryEnqueue(() =>
        {
            if (oldNick.Equals(Nickname, StringComparison.OrdinalIgnoreCase))
                Nickname = newNick;

            foreach (var kvp in _membersByChannel)
            {
                for (int i = 0; i < kvp.Value.Count; i++)
                {
                    var bare = kvp.Value[i].TrimStart('@', '%', '+');
                    if (bare.Equals(oldNick, StringComparison.OrdinalIgnoreCase))
                    {
                        var prefix = kvp.Value[i][..^bare.Length];
                        kvp.Value[i] = prefix + newNick;
                    }
                }
            }
            if (SelectedChannel != null)
                RefreshMemberList(SelectedChannel.Name);
        });
    }

    private void OnError(string error)
    {
        _dispatcher.TryEnqueue(() =>
        {
            ErrorMessage = error;
            if (!ShowConnectDialog)
                ShowConnectDialog = true;
        });
    }

    private void EnsureChannel(string channel)
    {
        if (FindChannel(channel) != null) return;

        var kind = channel.StartsWith('#') ? ChannelKind.Channel : ChannelKind.DirectMessage;
        var ch = new ChannelModel { Name = channel, Kind = kind };

        if (kind == ChannelKind.DirectMessage)
            DirectMessages.Add(ch);
        else
            Channels.Add(ch);
    }

    private ChannelModel? FindChannel(string name)
    {
        return Channels.FirstOrDefault(c => c.Name.Equals(name, StringComparison.OrdinalIgnoreCase))
            ?? DirectMessages.FirstOrDefault(c => c.Name.Equals(name, StringComparison.OrdinalIgnoreCase));
    }

    private void AddMessage(string channel, MessageModel msg)
    {
        if (!_messagesByChannel.ContainsKey(channel))
            _messagesByChannel[channel] = [];

        _messagesByChannel[channel].Add(msg);

        if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) == true)
            Messages.Add(msg);
    }

    private void AddSystemMessage(string channel, string text)
    {
        AddMessage(channel, new MessageModel
        {
            Id = Guid.NewGuid().ToString(),
            Nick = "*",
            Content = text,
            Timestamp = DateTimeOffset.Now,
        });
    }

    private void AddMemberToChannel(string channel, string nick)
    {
        if (!_membersByChannel.ContainsKey(channel))
            _membersByChannel[channel] = [];

        if (!_membersByChannel[channel].Any(n => n.TrimStart('@', '%', '+').Equals(nick, StringComparison.OrdinalIgnoreCase)))
            _membersByChannel[channel].Add(nick);

        if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) == true)
            RefreshMemberList(channel);
    }

    private void RemoveMemberFromChannel(string channel, string nick)
    {
        if (_membersByChannel.TryGetValue(channel, out var list))
            list.RemoveAll(n => n.TrimStart('@', '%', '+').Equals(nick, StringComparison.OrdinalIgnoreCase));

        if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) == true)
            RefreshMemberList(channel);
    }

    private static (MemberRole role, string nick) ParseMemberPrefix(string raw)
    {
        var role = MemberRole.Regular;
        var nick = raw.TrimStart('@', '%', '+');
        var prefix = raw[..^nick.Length];

        if (prefix.Contains('@')) role = MemberRole.Operator;
        else if (prefix.Contains('%')) role = MemberRole.HalfOp;
        else if (prefix.Contains('+')) role = MemberRole.Voiced;

        return (role, nick);
    }

    private void RefreshMemberList(string channel)
    {
        Operators.Clear();
        Voiced.Clear();
        Members.Clear();

        if (!_membersByChannel.TryGetValue(channel, out var nicks)) return;

        foreach (var raw in nicks.OrderBy(n => n.TrimStart('@', '%', '+')))
        {
            var (role, nick) = ParseMemberPrefix(raw);
            var member = new MemberModel { Nick = nick, Role = role, Did = _irc.GetAccount(nick) };

            switch (role)
            {
                case MemberRole.Operator:
                case MemberRole.HalfOp:
                    Operators.Add(member);
                    break;
                case MemberRole.Voiced:
                    Voiced.Add(member);
                    break;
                default:
                    Members.Add(member);
                    break;
            }
        }
    }

    public int TotalMemberCount => Operators.Count + Voiced.Count + Members.Count;

    public void SetSaslCredentials(string token, string did, string pdsUrl, string method)
    {
        _irc.SetSaslCredentials(token, did, pdsUrl, method);
    }
}
