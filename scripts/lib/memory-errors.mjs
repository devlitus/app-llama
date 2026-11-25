export class UsageError extends Error {
  constructor(message) {
    super(message);
    this.name = "UsageError";
    this.code = 1;
  }
}

export class PreconditionError extends Error {
  constructor(message) {
    super(message);
    this.name = "PreconditionError";
    this.code = 3;
  }
}

export class MemoryInputError extends Error {
  constructor(message) {
    super(message);
    this.name = "MemoryInputError";
    this.code = 2;
  }
}
