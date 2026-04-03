using Freeq.WinUI.ViewModels;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using System.Collections.ObjectModel;
using Windows.System;
using Windows.UI.Core;

namespace Freeq.WinUI.Controls;

public sealed partial class ComposeBox : UserControl
{
    private MainViewModel? _vm;

    // --- Nick autocomplete state ---
    private string? _acPrefix;           // the partial word we're completing
    private List<string>? _acMatches;   // candidate nicks
    private int _acIndex;               // which match is currently shown
    private bool _acActive;             // true while Tab cycling is ongoing

    public ComposeBox()
    {
        InitializeComponent();
        AutocompleteList.ItemsSource = new ObservableCollection<string>();
    }

    public void Bind(MainViewModel vm)
    {
        _vm = vm;

        vm.PropertyChanged += (s, e) =>
        {
            switch (e.PropertyName)
            {
                case nameof(MainViewModel.SelectedChannel):
                    MessageInput.PlaceholderText = vm.SelectedChannel != null
                        ? $"Message {vm.SelectedChannel.Name}"
                        : "Select a channel...";
                    break;

                case nameof(MainViewModel.TypingText):
                    TypingIndicator.Text = vm.TypingText;
                    TypingIndicator.Visibility = string.IsNullOrEmpty(vm.TypingText)
                        ? Visibility.Collapsed : Visibility.Visible;
                    break;

                case nameof(MainViewModel.IsEditing):
                    EditBar.Visibility = vm.IsEditing ? Visibility.Visible : Visibility.Collapsed;
                    break;

                case nameof(MainViewModel.ComposeText):
                    // Keep the TextBox in sync when the ViewModel sets ComposeText
                    // (e.g. BeginEditMessage fills in content from outside)
                    if (MessageInput.Text != vm.ComposeText)
                    {
                        MessageInput.Text = vm.ComposeText;
                        MessageInput.SelectionStart = MessageInput.Text.Length;
                    }
                    break;
            }
        };
    }

    // -----------------------------------------------------------------------
    // Key handling
    // -----------------------------------------------------------------------

    private void OnInputKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (_vm == null) return;

        bool shift = IsShiftDown();

        switch (e.Key)
        {
            case VirtualKey.Enter:
                if (shift)
                {
                    // Shift+Enter → insert newline; AcceptsReturn=True handles it automatically
                    // Don't mark as handled so the TextBox can process it.
                    _acActive = false;
                }
                else
                {
                    // Plain Enter → send message
                    Send();
                    e.Handled = true;
                }
                break;

            case VirtualKey.Up:
                // Up-arrow on empty input → begin editing last own message
                if (string.IsNullOrEmpty(MessageInput.Text) && !_vm.IsEditing)
                {
                    var last = _vm.GetLastOwnMessage();
                    if (last != null)
                    {
                        _vm.BeginEditMessage(last);
                        e.Handled = true;
                    }
                }
                break;

            case VirtualKey.Escape:
                if (_vm.IsEditing)
                {
                    _vm.CancelEditCommand.Execute(null);
                    e.Handled = true;
                }
                else if (AutocompletePopup.IsOpen)
                {
                    CloseAutocomplete();
                    e.Handled = true;
                }
                break;

            case VirtualKey.Tab:
                e.Handled = true;
                HandleTabAutocomplete();
                break;
        }
    }

    private void OnInputTextChanged(object sender, TextChangedEventArgs e)
    {
        if (_vm == null) return;

        // Sync text to ViewModel for the send command to read
        _vm.ComposeText = MessageInput.Text;

        // Cancel autocomplete state if user typed something other than Tab
        if (!_acActive)
            ResetAutocomplete();

        // Fire outbound typing indicator
        if (!string.IsNullOrWhiteSpace(MessageInput.Text))
            _vm.NotifyTyping();
    }

    // -----------------------------------------------------------------------
    // Send
    // -----------------------------------------------------------------------

    private void OnSendClick(object sender, RoutedEventArgs e)
    {
        Send();
    }

    private void Send()
    {
        if (_vm == null) return;
        _vm.ComposeText = MessageInput.Text;
        _vm.SendMessageCommand.Execute(null);
        MessageInput.Text = "";
        _acActive = false;
        ResetAutocomplete();
        MessageInput.Focus(FocusState.Programmatic);
    }

    // -----------------------------------------------------------------------
    // Edit mode
    // -----------------------------------------------------------------------

    private void OnCancelEditClick(object sender, RoutedEventArgs e)
    {
        _vm?.CancelEditCommand.Execute(null);
        MessageInput.Focus(FocusState.Programmatic);
    }

    // -----------------------------------------------------------------------
    // Tab autocomplete
    // -----------------------------------------------------------------------

    private void HandleTabAutocomplete()
    {
        if (_vm == null) return;

        var text = MessageInput.Text;
        var caretPos = MessageInput.SelectionStart;

        if (!_acActive)
        {
            // First Tab — figure out the partial word before the caret
            var wordStart = caretPos;
            while (wordStart > 0 && text[wordStart - 1] != ' ' && text[wordStart - 1] != '\n')
                wordStart--;

            var partial = text[wordStart..caretPos];
            if (string.IsNullOrEmpty(partial)) return;

            var nicks = _vm.GetCurrentChannelNicks();
            _acMatches = nicks
                .Where(n => n.StartsWith(partial, StringComparison.OrdinalIgnoreCase))
                .OrderBy(n => n)
                .ToList();

            if (_acMatches.Count == 0) return;

            _acPrefix = text[..wordStart];   // everything before the partial word
            _acIndex = 0;
            _acActive = true;
        }
        else
        {
            // Subsequent Tab — advance to next match
            _acIndex = (_acIndex + 1) % _acMatches!.Count;
        }

        // Apply match
        var match = _acMatches![_acIndex];
        var suffix = _acPrefix!.Length == 0 ? ": " : " "; // IRC convention: "nick: " at line start
        var rest = MessageInput.Text[(_acPrefix.Length + (MessageInput.SelectionStart - _acPrefix.Length))..];
        // Recompute: replace from _acPrefix end to the caret with the matched nick + suffix
        // Simple rebuild: prefix + nick + suffix + (any text after the old partial if needed)
        MessageInput.Text = _acPrefix + match + suffix;
        MessageInput.SelectionStart = MessageInput.Text.Length;
    }

    private void ResetAutocomplete()
    {
        _acPrefix = null;
        _acMatches = null;
        _acIndex = 0;
        _acActive = false;
        CloseAutocomplete();
    }

    private void CloseAutocomplete()
    {
        AutocompletePopup.IsOpen = false;
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    private static bool IsShiftDown()
    {
        try
        {
            var state = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift);
            return state.HasFlag(CoreVirtualKeyStates.Down);
        }
        catch
        {
            return false;
        }
    }
}
