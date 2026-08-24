using Microsoft.Gaming.XboxGameBar;
using System;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace AchievementWatcher.GameBar
{
    sealed partial class App : Application
    {
        private XboxGameBarWidget widget;

        public App()
        {
            InitializeComponent();
            Suspending += (_, __) => widget = null;
        }

        protected override void OnActivated(IActivatedEventArgs args)
        {
            var widgetArgs = args as XboxGameBarWidgetActivatedEventArgs;
            if (widgetArgs == null || !widgetArgs.IsLaunchActivation) return;

            var frame = new Frame();
            Window.Current.Content = frame;
            widget = new XboxGameBarWidget(widgetArgs, Window.Current.CoreWindow, frame);
            frame.Navigate(typeof(Widget), widget);
            Window.Current.Closed += (_, __) => widget = null;
            Window.Current.Activate();
        }

        protected override void OnLaunched(LaunchActivatedEventArgs args)
        {
            var frame = Window.Current.Content as Frame ?? new Frame();
            Window.Current.Content = frame;
            if (frame.Content == null) frame.Navigate(typeof(Widget), null);
            Window.Current.Activate();
        }
    }
}
