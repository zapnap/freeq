using Freeq.WinUI.Models;
using Freeq.WinUI.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;

namespace Freeq.WinUI.Controls;

public sealed partial class MessageList : UserControl
{
    private MainViewModel? _vm;

    public MessageList()
    {
        InitializeComponent();
    }

    public void Bind(MainViewModel vm)
    {
        _vm = vm;
        MessagesListView.ItemsSource = vm.Messages;

        vm.Messages.CollectionChanged += (s, e) =>
        {
            EmptyState.Visibility = vm.Messages.Count == 0 ? Visibility.Visible : Visibility.Collapsed;

            // Auto-scroll to bottom
            if (vm.Messages.Count > 0)
                MessagesListView.ScrollIntoView(vm.Messages[^1]);
        };

        vm.PropertyChanged += (s, e) =>
        {
            if (e.PropertyName == nameof(MainViewModel.SelectedChannel))
            {
                EmptyState.Visibility = vm.Messages.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
                EmptyChannelName.Text = vm.SelectedChannel?.DisplayName ?? "";
            }
        };
    }

    // -----------------------------------------------------------------------
    // Context menu (right-click on a message)
    // -----------------------------------------------------------------------

    private void OnMessageRightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if (_vm == null) return;

        var msg = FindMessageModel(e.OriginalSource as DependencyObject);
        if (msg == null) return;

        var flyout = new MenuFlyout();

        // Copy text
        var copyItem = new MenuFlyoutItem { Text = "Copy", Icon = new FontIcon { Glyph = "\uE8C8" } };
        copyItem.Click += (_, _) =>
        {
            var dp = new Windows.ApplicationModel.DataTransfer.DataPackage();
            dp.SetText(msg.Content);
            Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(dp);
        };
        flyout.Items.Add(copyItem);

        // Edit (only own messages, not deleted)
        if (msg.Nick.Equals(_vm.Nickname, StringComparison.OrdinalIgnoreCase) && !msg.IsDeleted)
        {
            flyout.Items.Add(new MenuFlyoutSeparator());
            var editItem = new MenuFlyoutItem { Text = "Edit", Icon = new FontIcon { Glyph = "\uE70F" } };
            editItem.Click += (_, _) => _vm.BeginEditMessage(msg);
            flyout.Items.Add(editItem);
        }

        // Delete (own messages, not already deleted)
        if (msg.Nick.Equals(_vm.Nickname, StringComparison.OrdinalIgnoreCase) && !msg.IsDeleted)
        {
            var deleteItem = new MenuFlyoutItem
            {
                Text = "Delete",
                Icon = new FontIcon { Glyph = "\uE74D" },
                Foreground = new SolidColorBrush(Windows.UI.Color.FromArgb(0xFF, 0xFF, 0x55, 0x55))
            };
            deleteItem.Click += (_, _) => _vm.DeleteMessage(msg);
            flyout.Items.Add(deleteItem);
        }

        flyout.ShowAt(MessagesListView, e.GetPosition(MessagesListView));
        e.Handled = true;
    }

    /// <summary>
    /// Walk up the visual tree from <paramref name="element"/> to find the first
    /// FrameworkElement whose Tag is a MessageModel (set via Tag="{x:Bind}" in the DataTemplate).
    /// </summary>
    private static MessageModel? FindMessageModel(DependencyObject? element)
    {
        while (element != null)
        {
            if (element is FrameworkElement fe && fe.Tag is MessageModel msg)
                return msg;
            element = VisualTreeHelper.GetParent(element);
        }
        return null;
    }

    // -----------------------------------------------------------------------
    // Static helpers for x:Bind
    // -----------------------------------------------------------------------

    public static Visibility BoolToVis(bool value) =>
        value ? Visibility.Visible : Visibility.Collapsed;

    public static Visibility BoolToVisInverse(bool value) =>
        value ? Visibility.Collapsed : Visibility.Visible;
}
