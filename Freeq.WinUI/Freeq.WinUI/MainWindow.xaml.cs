using Freeq.WinUI.Controls;
using Freeq.WinUI.Models;
using Freeq.WinUI.ViewModels;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Freeq.WinUI;

public sealed partial class MainWindow : Window
{
    private readonly MainViewModel _vm;

    public MainWindow()
    {
        InitializeComponent();

        Title = "Freeq";
        _vm = new MainViewModel(DispatcherQueue);

        // Bind controls to ViewModel
        SidebarControl.Bind(_vm);
        TopBarControl.Bind(_vm);
        MessageListControl.Bind(_vm);
        ComposeBoxControl.Bind(_vm);
        MemberListControl.Bind(_vm);

        // Wire events
        ConnectDialogControl.ConnectRequested += OnConnectRequested;
        SidebarControl.ChannelSelected += OnChannelSelected;
        SidebarControl.DisconnectRequested += OnDisconnectRequested;
        SidebarControl.JoinRequested += OnJoinRequested;
        TopBarControl.MemberListToggled += OnMemberListToggled;

        // Show/hide views based on connection state
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
        await _vm.ConnectCommand.ExecuteAsync(null);

        // Auto-join channels
        if (!string.IsNullOrWhiteSpace(request.Channels))
        {
            var channels = request.Channels.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            foreach (var channel in channels)
            {
                _vm.JoinChannelCommand.Execute(channel);
            }
        }
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
}
