namespace Freeq.WinUI.Models;

public class ReactionSummary
{
    public string Emoji { get; set; } = string.Empty;
    public int Count { get; set; }
    public bool IsMine { get; set; }

    public string Label => $"{Emoji} {Count}";
}
