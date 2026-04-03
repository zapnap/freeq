using Microsoft.UI.Xaml.Media;
using System.ComponentModel;

namespace Freeq.WinUI.Models;

public class MessageModel : INotifyPropertyChanged
{
    public event PropertyChangedEventHandler? PropertyChanged;

    private void Notify(string name) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));

    public string Id { get; set; } = string.Empty;
    public string Nick { get; set; } = string.Empty;
    public DateTimeOffset Timestamp { get; set; }
    public bool IsVerified { get; set; }
    public string? Did { get; set; }
    public bool IsSigned { get; set; }

    private string _content = string.Empty;
    public string Content
    {
        get => _content;
        set { _content = value; Notify(nameof(Content)); }
    }

    private bool _isEdited;
    public bool IsEdited
    {
        get => _isEdited;
        set { _isEdited = value; Notify(nameof(IsEdited)); }
    }

    private bool _isDeleted;
    public bool IsDeleted
    {
        get => _isDeleted;
        set { _isDeleted = value; Notify(nameof(IsDeleted)); }
    }

    public string TimeString => Timestamp.LocalDateTime.ToString("h:mm tt");
    public string DateString => Timestamp.LocalDateTime.ToString("MMMM d, yyyy");
    public string NickInitial => string.IsNullOrEmpty(Nick) ? "?" : Nick[..1].ToUpper();

    // Design system: hash-assigned nick color from the 5-hue palette
    public SolidColorBrush NickBrush => NickColor.ForNick(Nick);
    public SolidColorBrush AvatarBackgroundBrush => NickColor.AvatarBackgroundForNick(Nick);

    // Shortened DID for the verified badge chip (first 16 chars: "did:plc:abcdefgh")
    public string ShortDid => Did is { Length: > 16 } d ? d[..16] : Did ?? "";
    public bool HasDid => !string.IsNullOrEmpty(Did);
}
