import 'dart:async';
import 'dart:convert';
import 'package:http/http.dart' as http;

// Enum definition
enum TaskStatus { pending, inProgress, done }

// Mixin
mixin Timestamped {
  DateTime get createdAt;

  String get formattedDate =>
      '${createdAt.year}-${createdAt.month.toString().padLeft(2, '0')}-${createdAt.day.toString().padLeft(2, '0')}';
}

// Abstract class
abstract class Entity {
  final String id;

  const Entity(this.id);

  Map<String, dynamic> toJson();
}

// Generic class with mixin
class Task extends Entity with Timestamped {
  final String title;
  TaskStatus status;
  final List<String> tags;

  @override
  final DateTime createdAt;

  Task({
    required String id,
    required this.title,
    this.status = TaskStatus.pending,
    this.tags = const [],
    DateTime? createdAt,
  })  : createdAt = createdAt ?? DateTime.now(),
        super(id);

  bool get isCompleted => status == TaskStatus.done;

  Task copyWith({String? title, TaskStatus? status}) {
    return Task(
      id: id,
      title: title ?? this.title,
      status: status ?? this.status,
      tags: tags,
      createdAt: createdAt,
    );
  }

  @override
  Map<String, dynamic> toJson() => {
        'id': id,
        'title': title,
        'status': status.name,
        'tags': tags,
        'createdAt': createdAt.toIso8601String(),
      };

  factory Task.fromJson(Map<String, dynamic> json) => Task(
        id: json['id'] as String,
        title: json['title'] as String,
        status: TaskStatus.values.byName(json['status'] as String),
        tags: List<String>.from(json['tags'] as List),
        createdAt: DateTime.parse(json['createdAt'] as String),
      );

  @override
  String toString() => 'Task($id, "$title", ${status.name})';
}

// Generic repository interface
abstract class Repository<T extends Entity> {
  Future<List<T>> findAll();
  Future<T?> findById(String id);
  Future<void> save(T entity);
  Future<void> delete(String id);
}

// Concrete implementation with async
class TaskRepository implements Repository<Task> {
  final Map<String, Task> _store = {};

  @override
  Future<List<Task>> findAll() async => List.unmodifiable(_store.values);

  @override
  Future<Task?> findById(String id) async => _store[id];

  @override
  Future<void> save(Task task) async {
    _store[task.id] = task;
  }

  @override
  Future<void> delete(String id) async {
    _store.remove(id);
  }

  Future<List<Task>> findByStatus(TaskStatus status) async =>
      _store.values.where((t) => t.status == status).toList();
}

// Stream-based service
class TaskService {
  final TaskRepository _repo;
  final _controller = StreamController<List<Task>>.broadcast();

  TaskService(this._repo);

  Stream<List<Task>> get taskStream => _controller.stream;

  Future<void> addTask(String title, {List<String> tags = const []}) async {
    final task = Task(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      title: title,
      tags: tags,
    );
    await _repo.save(task);
    _controller.add(await _repo.findAll());
  }

  Future<void> complete(String id) async {
    final task = await _repo.findById(id);
    if (task == null) return;
    await _repo.save(task.copyWith(status: TaskStatus.done));
    _controller.add(await _repo.findAll());
  }

  void dispose() => _controller.close();
}

// Top-level function with pattern matching (Dart 3)
String describeTask(Task task) => switch (task.status) {
      TaskStatus.pending => 'Not started: ${task.title}',
      TaskStatus.inProgress => 'In progress: ${task.title}',
      TaskStatus.done => 'Completed: ${task.title}',
    };

// Extension
extension TaskListExtensions on List<Task> {
  List<Task> get completed => where((t) => t.isCompleted).toList();
  int get pendingCount =>
      where((t) => t.status == TaskStatus.pending).length;
}

Future<void> main() async {
  final repo = TaskRepository();
  final service = TaskService(repo);

  service.taskStream.listen((tasks) {
    print('Tasks updated: ${tasks.length} total');
  });

  await service.addTask('Buy groceries', tags: ['personal', 'errands']);
  await service.addTask('Write unit tests', tags: ['work']);
  await service.addTask('Read Dart docs', tags: ['learning']);

  final all = await repo.findAll();
  await service.complete(all.first.id);

  final pending = await repo.findByStatus(TaskStatus.pending);
  print('Pending: ${pending.map(describeTask).join(', ')}');

  final json = jsonEncode(all.map((t) => t.toJson()).toList());
  print('Serialized: $json');

  service.dispose();
}
