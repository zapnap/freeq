using System.Collections.Concurrent;
using System.Net.WebSockets;
using System.Text;

namespace Freeq.WinUI.Services;

public enum ConnectionState
{
    Disconnected,
    Connecting,
    Connected,
    Authenticating,
    Authenticated
}

public record IrcMessage(string? Prefix, string Command, string[] Params, Dictionary<string, string> Tags);

public class IrcClient : IDisposable
{
    private ClientWebSocket? _ws;
    private CancellationTokenSource? _cts;
    private readonly ConcurrentQueue<string> _sendQueue = new();

    public string Nick { get; private set; } = string.Empty;
    public string? Did { get; private set; }
    public ConnectionState State { get; private set; } = ConnectionState.Disconnected;

    public event Action<ConnectionState>? StateChanged;
    public event Action<string, string, string>? MessageReceived; // channel, nick, message
    public event Action<string, string>? JoinReceived; // channel, nick
    public event Action<string, string, string>? PartReceived; // channel, nick, reason
    public event Action<string, string>? QuitReceived; // nick, reason
    public event Action<string, string>? TopicReceived; // channel, topic
    public event Action<string, List<string>>? NamesReceived; // channel, nicks
    public event Action<string, string>? NickChanged; // oldNick, newNick
    public event Action<string>? RawReceived;
    public event Action<string>? ErrorReceived;

    // Caps the web app requests (matching freeq-app/src/irc/client.ts)
    private static readonly string[] WantedCaps =
    [
        "message-tags",
        "server-time",
        "batch",
        "multi-prefix",
        "echo-message",
        "account-notify",
        "extended-join",
        "away-notify",
        "draft/chathistory",
    ];

    private readonly HashSet<string> _ackedCaps = new();

    public async Task ConnectAsync(string url, string nick, string? password = null)
    {
        Nick = nick;
        SetState(ConnectionState.Connecting);

        _cts = new CancellationTokenSource();
        _ws = new ClientWebSocket();
        // No subprotocol — matches the web app's plain WebSocket connection

        try
        {
            await _ws.ConnectAsync(new Uri(url), _cts.Token);
            SetState(ConnectionState.Connected);

            _ = Task.Run(() => ReceiveLoop(_cts.Token));
            _ = Task.Run(() => SendLoop(_cts.Token));

            // IRC registration — same sequence as freeq-app client.ts
            Send("CAP LS 302");
            if (!string.IsNullOrEmpty(password))
                Send($"PASS {password}");
            Send($"NICK {nick}");
            Send($"USER {nick} 0 * :freeq desktop app");
        }
        catch (Exception ex)
        {
            ErrorReceived?.Invoke($"Connection failed: {ex.Message}");
            SetState(ConnectionState.Disconnected);
        }
    }

    public void Send(string raw)
    {
        _sendQueue.Enqueue(raw);
    }

    public void SendMessage(string target, string message)
    {
        Send($"PRIVMSG {target} :{message}");
    }

    public void JoinChannel(string channel)
    {
        Send($"JOIN {channel}");
    }

    public void PartChannel(string channel, string? reason = null)
    {
        Send(reason != null ? $"PART {channel} :{reason}" : $"PART {channel}");
    }

    public void Disconnect()
    {
        _cts?.Cancel();
        _ws?.Dispose();
        _ws = null;
        SetState(ConnectionState.Disconnected);
    }

    private void SetState(ConnectionState state)
    {
        State = state;
        StateChanged?.Invoke(state);
    }

    private async Task SendLoop(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested && _ws?.State == WebSocketState.Open)
        {
            if (_sendQueue.TryDequeue(out var msg))
            {
                var bytes = Encoding.UTF8.GetBytes(msg + "\r\n");
                await _ws.SendAsync(bytes, WebSocketMessageType.Text, true, ct);
            }
            else
            {
                await Task.Delay(10, ct);
            }
        }
    }

    private async Task ReceiveLoop(CancellationToken ct)
    {
        var buffer = new byte[8192];
        var sb = new StringBuilder();

        while (!ct.IsCancellationRequested && _ws?.State == WebSocketState.Open)
        {
            try
            {
                var result = await _ws.ReceiveAsync(buffer, ct);
                if (result.MessageType == WebSocketMessageType.Close)
                {
                    SetState(ConnectionState.Disconnected);
                    return;
                }

                sb.Append(Encoding.UTF8.GetString(buffer, 0, result.Count));

                // Process complete lines (may receive partial frames)
                var text = sb.ToString();
                int lastNewline = text.LastIndexOf('\n');
                if (lastNewline >= 0)
                {
                    var complete = text[..(lastNewline + 1)];
                    sb.Clear();
                    if (lastNewline + 1 < text.Length)
                        sb.Append(text[(lastNewline + 1)..]);

                    var lines = complete.Split('\n', StringSplitOptions.RemoveEmptyEntries);
                    foreach (var line in lines)
                        ProcessLine(line.TrimEnd('\r'));
                }
            }
            catch (OperationCanceledException) { break; }
            catch (Exception ex)
            {
                ErrorReceived?.Invoke($"Receive error: {ex.Message}");
                SetState(ConnectionState.Disconnected);
                break;
            }
        }
    }

    private void ProcessLine(string line)
    {
        if (string.IsNullOrWhiteSpace(line)) return;

        RawReceived?.Invoke(line);
        var msg = ParseIrcMessage(line);

        switch (msg.Command)
        {
            case "PING":
                Send($"PONG :{(msg.Params.Length > 0 ? msg.Params[0] : "")}");
                break;

            case "001": // RPL_WELCOME
                // Update nick to what the server assigned
                if (msg.Params.Length >= 1)
                    Nick = msg.Params[0];
                SetState(ConnectionState.Authenticated);
                break;

            case "PRIVMSG":
                if (msg.Params.Length >= 2)
                {
                    var nick = msg.Prefix?.Split('!')[0] ?? "unknown";
                    // Skip echo-message from ourselves if echo-message cap is active
                    if (_ackedCaps.Contains("echo-message") && nick.Equals(Nick, StringComparison.OrdinalIgnoreCase))
                        break;
                    MessageReceived?.Invoke(msg.Params[0], nick, msg.Params[1]);
                }
                break;

            case "JOIN":
                if (msg.Params.Length >= 1)
                {
                    var nick = msg.Prefix?.Split('!')[0] ?? "unknown";
                    JoinReceived?.Invoke(msg.Params[0], nick);
                }
                break;

            case "PART":
                if (msg.Params.Length >= 1)
                {
                    var nick = msg.Prefix?.Split('!')[0] ?? "unknown";
                    var reason = msg.Params.Length >= 2 ? msg.Params[1] : "";
                    PartReceived?.Invoke(msg.Params[0], nick, reason);
                }
                break;

            case "QUIT":
            {
                var nick = msg.Prefix?.Split('!')[0] ?? "unknown";
                var reason = msg.Params.Length >= 1 ? msg.Params[0] : "";
                QuitReceived?.Invoke(nick, reason);
                break;
            }

            case "332": // RPL_TOPIC
                if (msg.Params.Length >= 3)
                    TopicReceived?.Invoke(msg.Params[1], msg.Params[2]);
                break;

            case "TOPIC":
                if (msg.Params.Length >= 2)
                    TopicReceived?.Invoke(msg.Params[0], msg.Params[1]);
                break;

            case "353": // RPL_NAMREPLY
                if (msg.Params.Length >= 4)
                {
                    var nicks = msg.Params[3].Split(' ', StringSplitOptions.RemoveEmptyEntries).ToList();
                    NamesReceived?.Invoke(msg.Params[2], nicks);
                }
                break;

            case "NICK":
            {
                var oldNick = msg.Prefix?.Split('!')[0] ?? "";
                var newNick = msg.Params.Length >= 1 ? msg.Params[0] : "";
                NickChanged?.Invoke(oldNick, newNick);
                if (oldNick.Equals(Nick, StringComparison.OrdinalIgnoreCase))
                    Nick = newNick;
                break;
            }

            case "CAP":
                HandleCap(msg);
                break;

            case "903": // SASL success
                Send("CAP END");
                SetState(ConnectionState.Authenticated);
                break;

            case "904": // SASL failure
                Send("CAP END");
                ErrorReceived?.Invoke("SASL authentication failed");
                break;
        }
    }

    private void HandleCap(IrcMessage msg)
    {
        if (msg.Params.Length < 2) return;

        var subcommand = msg.Params[1];
        if (subcommand == "LS")
        {
            // Join all remaining params to get the full caps string (handles multi-line LS)
            var available = string.Join(" ", msg.Params.Skip(2));
            var requested = new List<string>();

            foreach (var cap in WantedCaps)
            {
                if (available.Contains(cap))
                    requested.Add(cap);
            }

            if (requested.Count > 0)
                Send($"CAP REQ :{string.Join(" ", requested)}");
            else
                Send("CAP END");
        }
        else if (subcommand == "ACK")
        {
            // Track which caps were acknowledged
            if (msg.Params.Length >= 3)
            {
                var acked = msg.Params[^1].Split(' ', StringSplitOptions.RemoveEmptyEntries);
                foreach (var cap in acked)
                    _ackedCaps.Add(cap);
            }

            // No SASL in guest mode, end cap negotiation
            Send("CAP END");
        }
        else if (subcommand == "NAK")
        {
            Send("CAP END");
        }
    }

    private static IrcMessage ParseIrcMessage(string line)
    {
        var tags = new Dictionary<string, string>();
        var pos = 0;

        // Parse tags
        if (line.StartsWith('@'))
        {
            var spaceIdx = line.IndexOf(' ');
            if (spaceIdx < 0) return new IrcMessage(null, line, [], tags);
            var tagStr = line[1..spaceIdx];
            foreach (var tag in tagStr.Split(';'))
            {
                var eqIdx = tag.IndexOf('=');
                if (eqIdx >= 0)
                    tags[tag[..eqIdx]] = tag[(eqIdx + 1)..];
                else
                    tags[tag] = "";
            }
            pos = spaceIdx + 1;
        }

        // Skip whitespace
        while (pos < line.Length && line[pos] == ' ') pos++;

        // Parse prefix
        string? prefix = null;
        if (pos < line.Length && line[pos] == ':')
        {
            var spaceIdx = line.IndexOf(' ', pos);
            if (spaceIdx < 0) return new IrcMessage(line[(pos + 1)..], "", [], tags);
            prefix = line[(pos + 1)..spaceIdx];
            pos = spaceIdx + 1;
        }

        while (pos < line.Length && line[pos] == ' ') pos++;

        // Parse command
        var cmdEnd = line.IndexOf(' ', pos);
        string command;
        if (cmdEnd < 0)
        {
            command = line[pos..];
            return new IrcMessage(prefix, command, [], tags);
        }
        command = line[pos..cmdEnd];
        pos = cmdEnd + 1;

        // Parse params
        var parms = new List<string>();
        while (pos < line.Length)
        {
            while (pos < line.Length && line[pos] == ' ') pos++;
            if (pos >= line.Length) break;

            if (line[pos] == ':')
            {
                parms.Add(line[(pos + 1)..]);
                break;
            }

            var nextSpace = line.IndexOf(' ', pos);
            if (nextSpace < 0)
            {
                parms.Add(line[pos..]);
                break;
            }
            parms.Add(line[pos..nextSpace]);
            pos = nextSpace + 1;
        }

        return new IrcMessage(prefix, command, [.. parms], tags);
    }

    public void Dispose()
    {
        _cts?.Cancel();
        _ws?.Dispose();
        GC.SuppressFinalize(this);
    }
}
