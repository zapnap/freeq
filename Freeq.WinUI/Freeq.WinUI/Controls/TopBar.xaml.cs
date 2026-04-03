using System;
using System.Linq;
using Freeq.WinUI.Models;
using Freeq.WinUI.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Freeq.WinUI.Controls;

public sealed partial class TopBar : UserControl
{
    public MainViewModel? ViewModel { get; set; }
    public event Action? MemberListToggled;

    public TopBar()
    {
        InitializeComponent();
    }

    public void Bind(MainViewModel vm)
    {
        ViewModel = vm;
        vm.PropertyChanged += (s, e) =>
        {
            switch (e.PropertyName)
            {
                case nameof(MainViewModel.SelectedChannel):
                    UpdateChannel(vm.SelectedChannel);
                    break;
                case nameof(MainViewModel.CurrentTopic):
                    TopicText.Text = vm.CurrentTopic;
                    break;
            }
        };

        vm.Operators.CollectionChanged += (s, e) => UpdateMemberCount();
        vm.Voiced.CollectionChanged += (s, e) => UpdateMemberCount();
        vm.Members.CollectionChanged += (s, e) => UpdateMemberCount();
    }

    private void UpdateChannel(ChannelModel? channel)
    {
        if (channel == null)
        {
            ChannelName.Text = "";
            TopicText.Text = "";
            ChannelIcon.Text = "";
            return;
        }

        ChannelIcon.Text = channel.Kind == ChannelKind.DirectMessage ? "\U0001F4AC" : "#";
        ChannelName.Text = channel.DisplayName;
        TopicText.Text = channel.Topic;
    }

    private void UpdateMemberCount()
    {
        if (ViewModel == null) return;
        MemberCountText.Text = ViewModel.TotalMemberCount.ToString();
    }

    private void OnMemberToggleClick(object sender, RoutedEventArgs e)
    {
        MemberListToggled?.Invoke();
    }

    private async void OnTopicClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel?.SelectedChannel == null) return;

        var input = new TextBox
        {
            Text = ViewModel.CurrentTopic,
            PlaceholderText = "Channel topic",
            Width = 400,
        };

        var dialog = new ContentDialog
        {
            Title = $"Set Topic for {ViewModel.SelectedChannel.Name}",
            PrimaryButtonText = "Set",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = this.XamlRoot,
            Content = input,
        };

        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
            ViewModel.SetTopicCommand.Execute(input.Text);
    }

    private async void OnPinsClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel?.SelectedChannel == null) return;

        // Clear previous results and request fresh pins from server
        ViewModel.RequestPinsCommand.Execute(null);

        // Give the server a moment to respond before showing the dialog
        await Task.Delay(300);

        var listView = new ListView
        {
            ItemsSource = ViewModel.PinnedMessages,
            MinHeight = 80,
            MaxHeight = 400,
        };
        listView.ItemTemplate = BuildPinTemplate();

        var emptyHint = new TextBlock
        {
            Text = "No pinned messages.",
            Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Gray),
            Visibility = ViewModel.PinnedMessages.Count == 0 ? Visibility.Visible : Visibility.Collapsed,
        };
        ViewModel.PinnedMessages.CollectionChanged += (_, _) =>
        {
            emptyHint.Visibility = ViewModel.PinnedMessages.Count == 0
                ? Visibility.Visible : Visibility.Collapsed;
        };

        var panel = new StackPanel { Spacing = 4 };
        panel.Children.Add(emptyHint);
        panel.Children.Add(listView);

        var dialog = new ContentDialog
        {
            Title = $"Pinned Messages — {ViewModel.SelectedChannel.Name}",
            CloseButtonText = "Close",
            XamlRoot = this.XamlRoot,
            Content = panel,
        };

        await dialog.ShowAsync();
    }

    private static DataTemplate BuildPinTemplate()
    {
        return (DataTemplate)Microsoft.UI.Xaml.Markup.XamlReader.Load("""
            <DataTemplate xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
                <StackPanel Padding="4,6" Spacing="2">
                    <TextBlock Text="{Binding Label}" FontWeight="SemiBold" />
                    <TextBlock Text="{Binding PinnedAtDisplay}" Foreground="Gray" FontSize="11" />
                </StackPanel>
            </DataTemplate>
            """);
    }
}
