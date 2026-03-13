#import <Foundation/Foundation.h>
#import "AppDelegate.h"

// ── Constants and macros ────────────────────────────────────────────────────

#define MAX_RETRIES 3
#define CLAMP(x, lo, hi) (((x) < (lo)) ? (lo) : (((x) > (hi)) ? (hi) : (x)))

static NSString *const kAppName    = @"Fulgur";
static NSString *const kVersion    = @"1.0.0";
static const NSTimeInterval kTimeout = 30.0;

typedef NS_ENUM(NSInteger, TaskPriority) {
    TaskPriorityLow,
    TaskPriorityNormal,
    TaskPriorityHigh,
    TaskPriorityCritical
};

typedef NS_OPTIONS(NSUInteger, TaskFlags) {
    TaskFlagNone      = 0,
    TaskFlagArchived  = 1 << 0,
    TaskFlagFavorite  = 1 << 1,
    TaskFlagPinned    = 1 << 2,
};

// ── Protocol ────────────────────────────────────────────────────────────────

@protocol Serializable <NSObject>

@required
- (NSDictionary *)toDictionary;

@optional
- (NSString *)toJSON;

@end

// ── Interface ───────────────────────────────────────────────────────────────

@interface Task : NSObject <Serializable, NSCopying>

@property (nonatomic, copy, readonly) NSString *identifier;
@property (nonatomic, copy) NSString *title;
@property (nonatomic, assign) TaskPriority priority;
@property (nonatomic, assign) TaskFlags flags;
@property (nonatomic, strong) NSDate *createdAt;
@property (nonatomic, weak) id<Serializable> parent;

+ (instancetype)taskWithTitle:(NSString *)title priority:(TaskPriority)priority;
- (BOOL)isOverdue;

@end

// ── Implementation ──────────────────────────────────────────────────────────

@implementation Task

+ (instancetype)taskWithTitle:(NSString *)title priority:(TaskPriority)priority {
    Task *task = [[self alloc] init];
    task->_identifier = [[NSUUID UUID] UUIDString];
    task.title = title;
    task.priority = priority;
    task.createdAt = [NSDate date];
    task.flags = TaskFlagNone;
    return task;
}

- (BOOL)isOverdue {
    NSTimeInterval elapsed = -[self.createdAt timeIntervalSinceNow];
    return elapsed > kTimeout;
}

- (NSDictionary *)toDictionary {
    return @{
        @"id":        self.identifier,
        @"title":     self.title ?: @"",
        @"priority":  @(self.priority),
        @"flags":     @(self.flags),
        @"createdAt": self.createdAt.description ?: [NSNull null],
    };
}

- (id)copyWithZone:(NSZone *)zone {
    Task *copy = [[[self class] allocWithZone:zone] init];
    copy->_identifier = [self.identifier copyWithZone:zone];
    copy.title     = self.title;
    copy.priority  = self.priority;
    copy.flags     = self.flags;
    copy.createdAt = [self.createdAt copy];
    return copy;
}

- (NSString *)description {
    return [NSString stringWithFormat:@"<Task: %@ \"%@\" priority=%ld>",
            self.identifier, self.title, (long)self.priority];
}

- (BOOL)isEqual:(id)object {
    if (![object isKindOfClass:[Task class]]) return NO;
    return [self.identifier isEqualToString:((Task *)object).identifier];
}

- (NSUInteger)hash {
    return self.identifier.hash;
}

@end

// ── Categories ──────────────────────────────────────────────────────────────

@interface NSArray (TaskFiltering)
- (NSArray<Task *> *)tasksWithPriority:(TaskPriority)priority;
@end

@implementation NSArray (TaskFiltering)

- (NSArray<Task *> *)tasksWithPriority:(TaskPriority)priority {
    NSPredicate *predicate = [NSPredicate predicateWithFormat:@"priority == %ld", (long)priority];
    return [self filteredArrayUsingPredicate:predicate];
}

@end

// ── Blocks and GCD ──────────────────────────────────────────────────────────

typedef void (^CompletionHandler)(NSArray<Task *> *_Nullable tasks, NSError *_Nullable error);

void fetchTasksAsync(CompletionHandler completion) {
    dispatch_async(dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT, 0), ^{
        NSMutableArray *tasks = [NSMutableArray array];
        NSArray *titles = @[@"Design UI", @"Write tests", @"Fix bug #42", @"Deploy"];
        for (NSString *title in titles) {
            [tasks addObject:[Task taskWithTitle:title priority:TaskPriorityNormal]];
        }

        dispatch_async(dispatch_get_main_queue(), ^{
            completion([tasks copy], nil);
        });
    });
}

// ── Main ────────────────────────────────────────────────────────────────────

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        Task *task = [Task taskWithTitle:@"Launch app" priority:TaskPriorityHigh];
        task.flags = TaskFlagFavorite | TaskFlagPinned;
        NSLog(@"%@ — overdue: %@", task, task.isOverdue ? @"YES" : @"NO");

        NSDictionary *dict = [task toDictionary];
        NSData *json = [NSJSONSerialization dataWithJSONObject:dict options:NSJSONWritingPrettyPrinted error:nil];
        NSLog(@"JSON:\n%@", [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding]);

        @try {
            SEL sel = @selector(toDictionary);
            if ([task respondsToSelector:sel]) {
                id result = [task performSelector:sel];
                NSLog(@"Selector result: %@", result);
            }
        } @catch (NSException *exception) {
            NSLog(@"Exception: %@ — %@", exception.name, exception.reason);
        } @finally {
            NSLog(@"Done. App: %@ v%@", kAppName, kVersion);
        }
    }
    return 0;
}
