using System.Linq;
using Freeq.WinUI.Controls;
using Freeq.WinUI.Models;
using Freeq.WinUI.ViewModels;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace Freeq.WinUI;

public sealed partial class MainWindow : Window
{
    private readonly MainViewModel _vm;

    public MainWindow()
    {
        InitializeComponent();

        Title = "Freeq";
        _vm = new MainViewModel(DispatcherQueue);

        SidebarControl.Bind(_vm);
        TopBarControl.Bind(_vm);
        MessageListControl.Bind(_vm);
        ComposeBoxControl.Bind(_vm);
        MemberListControl.Bind(_vm);

        ConnectDialogControl.ConnectRequested += OnConnectRequested;
        SidebarControl.ChannelSelected += OnChannelSelected;
        SidebarControl.DisconnectRequested += OnDisconnectRequested;
        SidebarControl.JoinRequested += OnJoinRequested;
        SidebarControl.MuteToggleRequested += OnMuteToggleRequested;
        TopBarControl.MemberListToggled += OnMemberListToggled;

        RegisterKeyboardShortcuts();

        _vm.PropertyChanged += (s, e) =>
        {
            if (e.PropertyName == nameof(MainViewModel.ShowConnectDialog))
            {
                ConnectDialogControl.Visibility = _vm.ShowConnectDialog
                    ? Visibility.Visible : Visibility.Collapsed;
                MainLayout.Visibility = _vm.ShowConnectDialog
                    ? Visibility.Collapsed : Visibility.Visible;

                if (_vm.ShowConnectDialog)
                    ConnectDialogControl.SetConnecting(false);
            }
            else if (e.PropertyName == nameof(MainViewModel.IsMemberListVisible))
            {
                MemberListControl.Visibility = _vm.IsMemberListVisible
                    ? Visibility.Visible : Visibility.Collapsed;
            }
            else if (e.PropertyName == nameof(MainViewModel.ErrorMessage) && _vm.ErrorMessage != null)
            {
                ConnectDialogControl.ShowError(_vm.ErrorMessage);
                ConnectDialogControl.SetConnecting(false);
            }
        };
    }

    private async void OnConnectRequested(ConnectRequest request)
    {
        _vm.ServerUrl = request.ServerUrl;
        _vm.Nickname = request.Nickname;

        // Set SASL credentials if AT Protocol login
        if (request.IsAtProto && !string.IsNullOrEmpty(request.SaslToken))
        {
            _vm.SetSaslCredentials(
                request.SaslToken,
                request.Did ?? "",
                request.PdsUrl ?? "",
                "web-token"
            );
        }

        // Queue channels BEFORE connecting — they'll be joined when 001 arrives
        if (!string.IsNullOrWhiteSpace(request.Channels))
        {
            var channels = request.Channels.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            _vm.QueueJoinChannels(channels);
        }

        await _vm.ConnectCommand.ExecuteAsync(null);
    }

    private void OnChannelSelected(ChannelModel channel)
    {
        _vm.SelectedChannel = channel;
    }

    private void OnDisconnectRequested()
    {
        _vm.DisconnectCommand.Execute(null);
    }

    private void OnJoinRequested(string channel)
    {
        _vm.JoinChannelCommand.Execute(channel);
    }

    private void OnMemberListToggled()
    {
        _vm.ToggleMemberListCommand.Execute(null);
    }

    private void OnMuteToggleRequested(ChannelModel channel)
    {
        _vm.ToggleMuteCommand.Execute(channel);
    }

    private void RegisterKeyboardShortcuts()
    {
        // In WinUI 3, Window doesn't inherit UIElement, so attach accelerators to Content (the root Grid).
        var root = (UIElement)this.Content;

        // Ctrl+K — quick channel switcher
        var quickSwitch = new KeyboardAccelerator
        {
            Key = VirtualKey.K,
            Modifiers = VirtualKeyModifiers.Control,
        };
        quickSwitch.Invoked += async (_, _) => await ShowQuickSwitcherAsync();
        root.KeyboardAccelerators.Add(quickSwitch);

        // Alt+Up — previous channel
        var altUp = new KeyboardAccelerator
        {
            Key = VirtualKey.Up,
            Modifiers = VirtualKeyModifiers.Menu,
        };
        altUp.Invoked += (_, _) => _vm.SelectPreviousChannel();
        root.KeyboardAccelerators.Add(altUp);

        // Alt+Down — next channel
        var altDown = new KeyboardAccelerator
        {
            Key = VirtualKey.Down,
            Modifiers = VirtualKeyModifiers.Menu,
        };
        altDown.Invoked += (_, _) => _vm.SelectNextChannel();
        root.KeyboardAccelerators.Add(altDown);

        // Alt+1…9, Alt+0 — jump to nth channel
        VirtualKey[] digitKeys =
        [
            VirtualKey.Number1, VirtualKey.Number2, VirtualKey.Number3,
            VirtualKey.Number4, VirtualKey.Number5, VirtualKey.Number6,
            VirtualKey.Number7, VirtualKey.Number8, VirtualKey.Number9,
            VirtualKey.Number0,
        ];
        for (int i = 0; i < digitKeys.Length; i++)
        {
            var digit = i + 1; // 1-based; 10 for index 9 (Alt+0)
            if (digit == 10) digit = 0; // Alt+0 → passed as 0
            var accel = new KeyboardAccelerator
            {
                Key = digitKeys[i],
                Modifiers = VirtualKeyModifiers.Menu,
            };
            var captured = digit;
            accel.Invoked += (_, _) => _vm.SelectChannelByIndex(captured);
            root.KeyboardAccelerators.Add(accel);
        }
    }

    private async System.Threading.Tasks.Task ShowQuickSwitcherAsync()
    {
        if (!_vm.IsConnected) return;

        var searchBox = new TextBox
        {
            PlaceholderText = "Jump to channel or DM…",
            Margin = new Thickness(0, 0, 0, 8),
        };

        var resultList = new ListView
        {
            SelectionMode = ListViewSelectionMode.Single,
            MaxHeight = 320,
        };

        void Refresh()
        {
            var filter = searchBox.Text;
            var all = _vm.Channels.Concat<ChannelModel>(_vm.DirectMessages);
            resultList.ItemsSource = (string.IsNullOrWhiteSpace(filter)
                ? all
                : all.Where(c => c.Name.Contains(filter, System.StringComparison.OrdinalIgnoreCase)))
                .ToList();

            if (resultList.Items.Count > 0)
                resultList.SelectedIndex = 0;
        }

        searchBox.TextChanged += (_, _) => Refresh();
        Refresh();

        var panel = new StackPanel { Spacing = 4 };
        panel.Children.Add(searchBox);
        panel.Children.Add(resultList);

        var dialog = new ContentDialog
        {
            Title = "Quick Switch",
            PrimaryButtonText = "Go",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = this.Content.XamlRoot,
            Content = panel,
        };

        // Enter in search box should confirm
        searchBox.KeyDown += (_, e) =>
        {
            if (e.Key == VirtualKey.Enter)
                dialog.Hide(); // triggers Primary path via keyboard
        };

        ChannelModel? selected = null;

        // Enter in search box selects first item and closes dialog as Primary
        searchBox.KeyDown += (_, e) =>
        {
            if (e.Key == VirtualKey.Enter && resultList.Items.Count > 0)
            {
                selected = resultList.SelectedItem as ChannelModel ?? resultList.Items[0] as ChannelModel;
                dialog.Hide();
            }
        };

        resultList.DoubleTapped += (_, _) =>
        {
            selected = resultList.SelectedItem as ChannelModel;
            dialog.Hide();
        };

        var result = await dialog.ShowAsync();
        var target = result == ContentDialogResult.Primary
            ? resultList.SelectedItem as ChannelModel
            : selected;

        if (target != null)
            _vm.SelectedChannel = target;
    }
}
