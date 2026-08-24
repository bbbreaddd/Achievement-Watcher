using Microsoft.Gaming.XboxGameBar;
using System;
using System.IO.Pipes;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Windows.Data.Json;
using Windows.Storage;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Navigation;

namespace AchievementWatcher.GameBar
{
    public sealed partial class Widget : Page
    {
        private const string PipeName = @"LOCAL\AchievementWatcher.GameBar.v1";
        private XboxGameBarWidgetNotificationManager notifications;
        private CancellationTokenSource cancellation;

        public Widget()
        {
            InitializeComponent();
            Token.Text = ApplicationData.Current.LocalSettings.Values["pairingToken"] as string ?? "";
        }

        protected override void OnNavigatedTo(NavigationEventArgs args)
        {
            var widget = args.Parameter as XboxGameBarWidget;
            if (widget != null)
            {
                notifications = new XboxGameBarWidgetNotificationManager(widget);
                if (!string.IsNullOrWhiteSpace(Token.Text)) StartBridge();
            }
        }

        private void Connect_Click(object sender, RoutedEventArgs args)
        {
            var value = Token.Text.Trim();
            if (value.Length != 64)
            {
                Status.Text = "The pairing token must be the 64-character value from Achievement Watcher.";
                return;
            }
            ApplicationData.Current.LocalSettings.Values["pairingToken"] = value;
            StartBridge();
        }

        private void StartBridge()
        {
            cancellation?.Cancel();
            cancellation = new CancellationTokenSource();
            _ = ListenAsync(cancellation.Token);
        }

        private async Task ListenAsync(CancellationToken cancellationToken)
        {
            while (!cancellationToken.IsCancellationRequested)
            {
                try
                {
                    using (var pipe = new NamedPipeClientStream(".", PipeName, PipeDirection.InOut, PipeOptions.Asynchronous))
                    {
                        await SetStatusAsync("Connecting to Achievement Watcher…");
                        await pipe.ConnectAsync(2000, cancellationToken);
                        pipe.ReadMode = PipeTransmissionMode.Message;
                        await WriteAsync(pipe, "{\"token\":\"" + Token.Text.Trim() + "\"}", cancellationToken);
                        await SetStatusAsync("Connected. Fullscreen notifications are ready.");
                        while (pipe.IsConnected && !cancellationToken.IsCancellationRequested)
                        {
                            var payload = await ReadAsync(pipe, cancellationToken);
                            var success = await ShowNotificationAsync(payload);
                            await WriteAsync(pipe, success ? "{\"success\":true}" : "{\"success\":false}", cancellationToken);
                        }
                    }
                }
                catch (OperationCanceledException) { return; }
                catch (Exception error)
                {
                    await SetStatusAsync("Disconnected: " + error.Message + ". Retrying…");
                    await Task.Delay(1500, cancellationToken);
                }
            }
        }

        private async Task<bool> ShowNotificationAsync(string payload)
        {
            if (notifications == null) return false;
            var root = JsonObject.Parse(payload);
            var observation = root.GetNamedObject("observation");
            var title = observation.GetNamedString("displayName", observation.GetNamedString("achievementId", "Achievement unlocked"));
            var content = observation.GetNamedString("description", "Achievement unlocked");
            return await Dispatcher.RunTaskAsync(async () =>
            {
                var notification = new XboxGameBarWidgetNotificationBuilder(title)
                    .Content(content)
                    .IsBackgroundActivation(true)
                    .BuildNotification();
                var result = await notifications.TryShowAsync(notification);
                return string.Equals(result.ToString(), "Succeeded", StringComparison.OrdinalIgnoreCase);
            });
        }

        private async Task SetStatusAsync(string value)
        {
            await Dispatcher.RunTaskAsync(async () =>
            {
                Status.Text = value;
                await Task.CompletedTask;
                return true;
            });
        }

        private static async Task<string> ReadAsync(NamedPipeClientStream pipe, CancellationToken token)
        {
            var buffer = new byte[65536];
            var count = await pipe.ReadAsync(buffer, 0, buffer.Length, token);
            if (count == 0) throw new InvalidOperationException("Watcher closed the pipe");
            return Encoding.UTF8.GetString(buffer, 0, count);
        }

        private static async Task WriteAsync(NamedPipeClientStream pipe, string value, CancellationToken token)
        {
            var bytes = Encoding.UTF8.GetBytes(value);
            await pipe.WriteAsync(bytes, 0, bytes.Length, token);
            await pipe.FlushAsync(token);
        }
    }

    internal static class DispatcherExtensions
    {
        internal static async Task<T> RunTaskAsync<T>(this CoreDispatcher dispatcher, Func<Task<T>> action)
        {
            var completion = new TaskCompletionSource<T>();
            await dispatcher.RunAsync(CoreDispatcherPriority.Normal, async () =>
            {
                try { completion.SetResult(await action()); }
                catch (Exception error) { completion.SetException(error); }
            });
            return await completion.Task;
        }
    }
}
