// BizbuzQuickActionsBridge.m
//
// Home Screen Quick Actions (long-press the app icon) need
// `UIApplication.delegate` to implement
// `application:performActionForShortcutItem:completionHandler:`, and ideally
// to read `UIApplicationLaunchOptionsShortcutItemKey` out of
// `application:didFinishLaunchingWithOptions:` for the cold-launch case.
//
// Tauri's iOS runtime (the `tao` crate) owns the concrete app delegate class
// and implements neither — there is no plugin-level extension point for this,
// so this file hooks in via Objective-C runtime swizzling instead:
//
//   1. Swizzle `-[UIApplication setDelegate:]` so the moment `tao` installs
//      its delegate instance, we learn its concrete class.
//   2. On that class: ADD `performActionForShortcutItem:` (tao doesn't
//      implement it, so this is a plain addition, not a swap).
//   3. On that class: SWIZZLE `didFinishLaunchingWithOptions:` (tao does
//      implement this, so we save the original IMP and call through to it
//      after extracting any launch-time shortcut item).
//
// Both paths funnel into the same "pending action" mailbox: an
// NSUserDefaults key, durable across the timing gap before the webview (and
// BizbuzQuickActionsPlugin.swift, which reads it back) exists at all. The
// JS side polls it once at boot and once on every resume rather than
// needing a live event channel, since this key covers both cases already.
//
// Compiled directly into the app target via project.yml's `../../ios-native`
// source path (patched by scripts/build-ios.cjs) and initialized from
// main.mm before `ffi::start_app()` runs — i.e. before `tao` ever calls
// `-setDelegate:`.

#import <UIKit/UIKit.h>
#import <objc/runtime.h>

static NSString * const kBizbuzPendingQuickActionDefaultsKey = @"BizbuzPendingQuickAction";

static void bizbuz_storePendingAction(NSString *actionId) {
    if (actionId.length == 0) return;
    [[NSUserDefaults standardUserDefaults] setObject:actionId forKey:kBizbuzPendingQuickActionDefaultsKey];
}

#pragma mark - performActionForShortcutItem: (added — tao has no implementation)

static void bizbuz_performActionForShortcutItem(id self, SEL _cmd, UIApplication *application,
                                                 UIApplicationShortcutItem *shortcutItem,
                                                 void (^completionHandler)(BOOL succeeded)) {
    bizbuz_storePendingAction(shortcutItem.type);
    if (completionHandler) completionHandler(YES);
}

#pragma mark - didFinishLaunchingWithOptions: (swizzled — tao already implements this)

static IMP bizbuz_originalDidFinishLaunchingIMP = NULL;
typedef BOOL (*BizbuzDidFinishLaunchingFn)(id, SEL, UIApplication *, NSDictionary *);

static BOOL bizbuz_didFinishLaunchingWithOptions(id self, SEL _cmd, UIApplication *application,
                                                  NSDictionary *launchOptions) {
    UIApplicationShortcutItem *shortcutItem = launchOptions[UIApplicationLaunchOptionsShortcutItemKey];
    if (shortcutItem) {
        bizbuz_storePendingAction(shortcutItem.type);
    }
    if (bizbuz_originalDidFinishLaunchingIMP) {
        BizbuzDidFinishLaunchingFn fn = (BizbuzDidFinishLaunchingFn)bizbuz_originalDidFinishLaunchingIMP;
        return fn(self, _cmd, application, launchOptions);
    }
    return YES;
}

#pragma mark - Install once the concrete delegate class is known

static void bizbuz_installOnDelegateClass(Class delegateClass) {
    static BOOL installed = NO;
    if (installed || delegateClass == Nil) return;
    installed = YES;

    SEL performActionSel = @selector(application:performActionForShortcutItem:completionHandler:);
    if (![delegateClass instancesRespondToSelector:performActionSel]) {
        class_addMethod(delegateClass, performActionSel, (IMP)bizbuz_performActionForShortcutItem, "v@:@@@?");
    }

    SEL didFinishSel = @selector(application:didFinishLaunchingWithOptions:);
    Method originalMethod = class_getInstanceMethod(delegateClass, didFinishSel);
    if (originalMethod) {
        bizbuz_originalDidFinishLaunchingIMP = method_getImplementation(originalMethod);
        method_setImplementation(originalMethod, (IMP)bizbuz_didFinishLaunchingWithOptions);
    } else {
        class_addMethod(delegateClass, didFinishSel, (IMP)bizbuz_didFinishLaunchingWithOptions, "B@:@@");
    }
}

#pragma mark - Swizzle UIApplication.setDelegate: to learn the delegate class dynamically

static IMP bizbuz_originalSetDelegateIMP = NULL;
typedef void (*BizbuzSetDelegateFn)(id, SEL, id<UIApplicationDelegate>);

static void bizbuz_setDelegate(id self, SEL _cmd, id<UIApplicationDelegate> delegate) {
    if (delegate) {
        bizbuz_installOnDelegateClass([delegate class]);
    }
    if (bizbuz_originalSetDelegateIMP) {
        BizbuzSetDelegateFn fn = (BizbuzSetDelegateFn)bizbuz_originalSetDelegateIMP;
        fn(self, _cmd, delegate);
    }
}

#pragma mark - Public init, called from main.mm before ffi::start_app()

void BizbuzQuickActionsBridgeInit(void) {
    Method setDelegateMethod = class_getInstanceMethod([UIApplication class], @selector(setDelegate:));
    if (setDelegateMethod && !bizbuz_originalSetDelegateIMP) {
        bizbuz_originalSetDelegateIMP = method_getImplementation(setDelegateMethod);
        method_setImplementation(setDelegateMethod, (IMP)bizbuz_setDelegate);
    }
}
