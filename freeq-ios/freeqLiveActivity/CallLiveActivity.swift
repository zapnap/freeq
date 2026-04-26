import ActivityKit
import SwiftUI
import WidgetKit

@main
struct FreeqWidgetBundle: WidgetBundle {
    var body: some Widget {
        CallLiveActivity()
    }
}

/// In-call Live Activity. Renders three surfaces:
///   - Lock-screen / banner
///   - Dynamic Island compact (off-screen) / minimal (multitasking) / expanded (long-press)
struct CallLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CallActivityAttributes.self) { context in
            // Lock-screen / banner view
            HStack(spacing: 12) {
                ZStack {
                    Circle()
                        .fill(Color.green.opacity(0.18))
                        .frame(width: 40, height: 40)
                    Image(systemName: "phone.fill")
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(.green)
                }

                VStack(alignment: .leading, spacing: 2) {
                    Text(context.attributes.channel)
                        .font(.system(size: 15, weight: .semibold))
                        .lineLimit(1)
                    HStack(spacing: 6) {
                        Text(timerInterval: context.state.startedAt...Date.distantFuture, countsDown: false)
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(.secondary)
                        Text("·").foregroundStyle(.secondary)
                        Text("\(context.state.participantCount) on call")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
                if context.state.isMuted {
                    Image(systemName: "mic.slash.fill")
                        .foregroundStyle(.orange)
                        .font(.system(size: 16))
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .activityBackgroundTint(Color.black)
            .activitySystemActionForegroundColor(Color.white)

        } dynamicIsland: { context in
            DynamicIsland {
                // Expanded — visible on long-press
                DynamicIslandExpandedRegion(.leading) {
                    Image(systemName: "phone.fill")
                        .foregroundStyle(.green)
                        .font(.system(size: 18, weight: .semibold))
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if context.state.isMuted {
                        Image(systemName: "mic.slash.fill")
                            .foregroundStyle(.orange)
                            .font(.system(size: 16))
                    } else {
                        Image(systemName: "mic.fill")
                            .foregroundStyle(.green)
                            .font(.system(size: 16))
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(spacing: 2) {
                        Text(context.attributes.channel)
                            .font(.system(size: 14, weight: .semibold))
                            .lineLimit(1)
                        Text("\(context.state.participantCount) on call")
                            .font(.system(size: 11))
                            .foregroundStyle(.secondary)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        Text(timerInterval: context.state.startedAt...Date.distantFuture, countsDown: false)
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(.secondary)
                        Spacer()
                        // Tapping anywhere falls through to deep-link → main app.
                        Text("Tap to open")
                            .font(.system(size: 11))
                            .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 4)
                }
            } compactLeading: {
                Image(systemName: "phone.fill")
                    .foregroundStyle(.green)
            } compactTrailing: {
                if context.state.isMuted {
                    Image(systemName: "mic.slash.fill")
                        .foregroundStyle(.orange)
                } else {
                    Text(timerInterval: context.state.startedAt...Date.distantFuture,
                         countsDown: false,
                         showsHours: false)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(.green)
                        .frame(maxWidth: 44)
                }
            } minimal: {
                Image(systemName: "phone.fill")
                    .foregroundStyle(.green)
            }
            .keylineTint(.green)
        }
    }
}
