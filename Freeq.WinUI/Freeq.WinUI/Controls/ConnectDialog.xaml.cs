using System;
using System.Net.Http;
using System.Text.Json;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.System;

namespace Freeq.WinUI.Controls;

public sealed partial class ConnectDialog : UserControl
{
    private const string BrokerBase = "https://auth.freeq.at";
    private static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(10) };

    private bool _isAtProtoMode = true;
    private bool _isConnecting;
    private bool _advancedVisible;
    private bool _nickManuallyEdited;

    public event Action<ConnectRequest>? ConnectRequested;

    public ConnectDialog()
    {
        InitializeComponent();
        var rng = new Random();
        GuestNickInput.Text = $"desktop{rng.Next(10000, 99999)}";
        SetMode(true);
    }

    public void ShowError(string message)
    {
        ErrorText.Text = message;
        ErrorBox.Visibility = Visibility.Visible;
    }

    public void SetConnecting(bool connecting)
    {
        _isConnecting = connecting;
        ConnectButton.IsEnabled = !connecting;
        ConnectSpinner.IsActive = connecting;
        ConnectSpinner.Visibility = connecting ? Visibility.Visible : Visibility.Collapsed;
        ConnectButtonText.Text = connecting
            ? (_isAtProtoMode ? "Waiting for authorization..." : "Connecting...")
            : (_isAtProtoMode ? "Sign in with AT Protocol" : "Connect as Guest");
    }

    private void SetMode(bool atProto)
    {
        _isAtProtoMode = atProto;

        // Tab styling
        AtProtoTab.Background = atProto
            ? new SolidColorBrush(ColorHelper.FromArgb(25, 0, 212, 170))
            : new SolidColorBrush(Colors.Transparent);
        AtProtoTab.Foreground = atProto
            ? new SolidColorBrush(ColorHelper.FromArgb(255, 0, 212, 170))
            : new SolidColorBrush(ColorHelper.FromArgb(255, 85, 85, 112));

        GuestTab.Background = !atProto
            ? new SolidColorBrush(ColorHelper.FromArgb(25, 0, 212, 170))
            : new SolidColorBrush(Colors.Transparent);
        GuestTab.Foreground = !atProto
            ? new SolidColorBrush(ColorHelper.FromArgb(255, 0, 212, 170))
            : new SolidColorBrush(ColorHelper.FromArgb(255, 85, 85, 112));

        // Show/hide fields
        AtProtoFields.Visibility = atProto ? Visibility.Visible : Visibility.Collapsed;
        GuestFields.Visibility = atProto ? Visibility.Collapsed : Visibility.Visible;

        // Button text
        ConnectButtonText.Text = atProto ? "Sign in with AT Protocol" : "Connect as Guest";

        // Focus
        if (atProto)
            HandleInput.Focus(FocusState.Programmatic);
        else
            GuestNickInput.Focus(FocusState.Programmatic);
    }

    private void OnAtProtoTabClick(object sender, RoutedEventArgs e) => SetMode(true);
    private void OnGuestTabClick(object sender, RoutedEventArgs e) => SetMode(false);

    private void OnHandleChanged(object sender, TextChangedEventArgs e)
    {
        if (_nickManuallyEdited) return;

        // Derive nick from handle (strip .bsky.social, etc.)
        var handle = HandleInput.Text.Trim();
        if (string.IsNullOrEmpty(handle))
        {
            AtNickInput.Text = "";
            return;
        }

        var nick = handle;
        // Strip common suffixes
        foreach (var suffix in new[] { ".bsky.social", ".bsky.network", ".bsky.app" })
        {
            if (nick.EndsWith(suffix, StringComparison.OrdinalIgnoreCase))
            {
                nick = nick[..^suffix.Length];
                break;
            }
        }

        // Clean for IRC nick validity
        nick = nick.Replace(".", "").Replace("-", "").Replace("_", "");
        if (nick.Length > 16) nick = nick[..16];

        AtNickInput.Text = nick;
    }

    private void OnAdvancedToggle(object sender, RoutedEventArgs e)
    {
        _advancedVisible = !_advancedVisible;
        AdvancedFields.Visibility = _advancedVisible ? Visibility.Visible : Visibility.Collapsed;
        AdvancedArrow.Text = _advancedVisible ? "\uE70D" : "\uE76C";
    }

    private void OnFieldKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Enter)
        {
            OnConnectClick(sender, e);
            e.Handled = true;
        }
    }

    private async void OnConnectClick(object sender, RoutedEventArgs e)
    {
        if (_isConnecting) return;

        ErrorBox.Visibility = Visibility.Collapsed;
        var server = ServerInput.Text.Trim();
        var channels = ChannelInput.Text.Trim();

        if (_isAtProtoMode)
        {
            var handle = HandleInput.Text.Trim();
            if (string.IsNullOrEmpty(handle))
            {
                ShowError("Please enter your AT Protocol handle.");
                return;
            }

            var nick = string.IsNullOrWhiteSpace(AtNickInput.Text)
                ? handle.Split('.')[0]
                : AtNickInput.Text.Trim();

            SetConnecting(true);

            try
            {
                // Check broker health
                var healthResp = await Http.GetAsync($"{BrokerBase}/health");
                if (!healthResp.IsSuccessStatusCode)
                {
                    ShowError("Authentication service is unavailable. Try guest mode.");
                    SetConnecting(false);
                    return;
                }

                // Launch browser for OAuth
                var returnTo = Uri.EscapeDataString("freeq-desktop://oauth-callback");
                var loginUrl = $"{BrokerBase}/auth/login?handle={Uri.EscapeDataString(handle)}&return_to={returnTo}";
                await Launcher.LaunchUriAsync(new Uri(loginUrl));

                // For now, fall back to guest connection after OAuth redirect
                // Full OAuth callback handling requires URI protocol registration
                // Connect as guest with the desired nick in the meantime
                ConnectRequested?.Invoke(new ConnectRequest
                {
                    ServerUrl = server,
                    Nickname = nick,
                    Channels = channels,
                    IsAtProto = true,
                    Handle = handle,
                });
            }
            catch (Exception ex)
            {
                ShowError($"Authentication failed: {ex.Message}");
                SetConnecting(false);
            }
        }
        else
        {
            // Guest mode
            var nick = GuestNickInput.Text.Trim();
            if (string.IsNullOrEmpty(nick))
            {
                ShowError("Please enter a nickname.");
                return;
            }

            SetConnecting(true);
            ConnectRequested?.Invoke(new ConnectRequest
            {
                ServerUrl = server,
                Nickname = nick,
                Channels = channels,
                IsAtProto = false,
            });
        }
    }
}

public class ConnectRequest
{
    public string ServerUrl { get; set; } = "";
    public string Nickname { get; set; } = "";
    public string Channels { get; set; } = "#freeq";
    public bool IsAtProto { get; set; }
    public string? Handle { get; set; }
    public string? SaslToken { get; set; }
    public string? Did { get; set; }
    public string? PdsUrl { get; set; }
}
