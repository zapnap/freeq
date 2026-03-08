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

    public ObservableCollection<ChannelModel> Channels { get; } = [];
    public ObservableCollection<ChannelModel> FavoriteChannels { get; } = [];
    public ObservableCollection<ChannelModel> DirectMessages { get; } = [];
    public ObservableCollection<MessageModel> Messages { get; } = [];
    public ObservableCollection<MemberModel> Operators { get; } = [];
    public ObservableCollection<MemberModel> Voiced { get; } = [];
    public ObservableCollection<MemberModel> Members { get; } = [];

    private readonly Dictionary<string, List<MessageModel>> _messagesByChannel = new(StringComparer.OrdinalIgnoreCase);
    private readonly Dictionary<string, List<string>> _membersByChannel = new(StringComparer.OrdinalIgnoreCase);

    public MainViewModel(DispatcherQueue dispatcher)
    {
        _dispatcher = dispatcher;

        _irc.StateChanged += OnStateChanged;
        _irc.MessageReceived += OnMessageReceived;
        _irc.JoinReceived += OnJoinReceived;
        _irc.PartReceived += OnPartReceived;
        _irc.QuitReceived += OnQuitReceived;
        _irc.TopicReceived += OnTopicReceived;
        _irc.NamesReceived += OnNamesReceived;
        _irc.NickChanged += OnNickChanged;
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
        ShowConnectDialog = false;
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
        SelectedChannel = null;
        ShowConnectDialog = true;
        IsConnected = false;
    }

    [RelayCommand]
    private void SendMessage()
    {
        if (string.IsNullOrWhiteSpace(ComposeText) || SelectedChannel == null) return;

        _irc.SendMessage(SelectedChannel.Name, ComposeText);

        var msg = new MessageModel
        {
            Id = Guid.NewGuid().ToString(),
            Nick = _irc.Nick,
            Content = ComposeText,
            Timestamp = DateTimeOffset.Now,
        };

        AddMessage(SelectedChannel.Name, msg);
        ComposeText = "";
    }

    [RelayCommand]
    private void JoinChannel(string channel)
    {
        if (string.IsNullOrWhiteSpace(channel)) return;
        if (!channel.StartsWith('#')) channel = "#" + channel;
        _irc.JoinChannel(channel);
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

            if (state == ConnectionState.Disconnected)
            {
                ShowConnectDialog = true;
            }
        });
    }

    private void OnMessageReceived(string channel, string nick, string message)
    {
        _dispatcher.TryEnqueue(() =>
        {
            EnsureChannel(channel);

            var msg = new MessageModel
            {
                Id = Guid.NewGuid().ToString(),
                Nick = nick,
                Content = message,
                Timestamp = DateTimeOffset.Now,
            };

            AddMessage(channel, msg);

            if (SelectedChannel?.Name != channel)
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

            _membersByChannel[channel].AddRange(nicks);
            if (SelectedChannel?.Name.Equals(channel, StringComparison.OrdinalIgnoreCase) == true)
                RefreshMemberList(channel);
        });
    }

    private void OnNickChanged(string oldNick, string newNick)
    {
        _dispatcher.TryEnqueue(() =>
        {
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
        _dispatcher.TryEnqueue(() => ErrorMessage = error);
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

    private void RefreshMemberList(string channel)
    {
        Operators.Clear();
        Voiced.Clear();
        Members.Clear();

        if (!_membersByChannel.TryGetValue(channel, out var nicks)) return;

        foreach (var raw in nicks.OrderBy(n => n.TrimStart('@', '%', '+')))
        {
            var role = MemberRole.Regular;
            var nick = raw;

            if (raw.StartsWith('@')) { role = MemberRole.Operator; nick = raw[1..]; }
            else if (raw.StartsWith('%')) { role = MemberRole.HalfOp; nick = raw[1..]; }
            else if (raw.StartsWith('+')) { role = MemberRole.Voiced; nick = raw[1..]; }

            var member = new MemberModel { Nick = nick, Role = role };

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
}
