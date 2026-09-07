---
name: backend-dev/nodejs
description: Node.js/Express/Fastify backend development — project structure, middleware patterns, route design, validation, WebSocket, background jobs, logging, graceful shutdown, cluster mode, environment config, health checks, API versioning.
---

# Node.js Backend Development

## Project Structure

```
src/
├── config/
│   ├── index.ts          # Environment config with validation
│   ├── database.ts       # DB connection config
│   └── redis.ts          # Redis config
├── middleware/
│   ├── error-handler.ts  # Global error handler
│   ├── auth.ts           # Authentication middleware
│   ├── rate-limiter.ts   # Rate limiting
│   ├── cors.ts           # CORS configuration
│   ├── compression.ts    # Response compression
│   ├── request-id.ts     # Correlation ID
│   └── validate.ts       # Request validation
├── modules/
│   └── [feature]/
│       ├── routes.ts     # Route definitions
│       ├── controller.ts # Request handlers
│       ├── service.ts    # Business logic
│       ├── repository.ts # Data access
│       ├── model.ts      # Type definitions
│       └── __tests__/    # Tests
├── utils/
│   ├── logger.ts         # Winston/Pino logger
│   ├── errors.ts         # Custom error classes
│   └── helpers.ts        # Utility functions
├── jobs/
│   └── worker.ts         # Background job processors
├── app.ts                # Express/Fastify app setup
└── server.ts             # Server entry point
```

## Express Application Setup

```typescript
// app.ts
import express from 'express';
import cors from 'cors';
import compression from 'compression';
import helmet from 'helmet';
import { v4 as uuidv4 } from 'uuid';
import { errorHandler } from './middleware/error-handler';
import { rateLimiter } from './middleware/rate-limiter';
import { logger } from './utils/logger';
import { routes } from './modules/routes';

export function createApp() {
  const app = express();

  // Security
  app.use(helmet());
  app.use(cors({
    origin: process.env.CORS_ORIGIN?.split(',') || '*',
    credentials: true,
    methods: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'],
    allowedHeaders: ['Content-Type', 'Authorization', 'X-Request-ID'],
  }));

  // Compression
  app.use(compression({ threshold: 1024 }));

  // Request ID
  app.use((req, res, next) => {
    req.id = req.headers['x-request-id'] as string || uuidv4();
    res.setHeader('X-Request-ID', req.id);
    next();
  });

  // Body parsing
  app.use(express.json({ limit: '10mb' }));
  app.use(express.urlencoded({ extended: true }));

  // Rate limiting
  app.use(rateLimiter);

  // Routes
  app.use('/api/v1', routes);

  // Health check
  app.get('/health', (req, res) => {
    res.json({ status: 'ok', timestamp: new Date().toISOString() });
  });

  // Error handler (must be last)
  app.use(errorHandler);

  return app;
}
```

```typescript
// server.ts
import { createApp } from './app';
import { logger } from './utils/logger';
import { config } from './config';
import { connectDatabase } from './config/database';
import { connectRedis } from './config/redis';

async function bootstrap() {
  await connectDatabase();
  await connectRedis();

  const app = createApp();
  const server = app.listen(config.port, () => {
    logger.info(`Server running on port ${config.port}`);
  });

  // Graceful shutdown
  const shutdown = async (signal: string) => {
    logger.info(`${signal} received, starting graceful shutdown`);
    server.close(async () => {
      await closeDatabase();
      await closeRedis();
      logger.info('Server shut down');
      process.exit(0);
    });

    // Force shutdown after 30s
    setTimeout(() => {
      logger.error('Forced shutdown after timeout');
      process.exit(1);
    }, 30000);
  };

  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));

  // Unhandled errors
  process.on('unhandledRejection', (reason) => {
    logger.error('Unhandled rejection', { reason });
  });

  process.on('uncaughtException', (error) => {
    logger.error('Uncaught exception', { error });
    process.exit(1);
  });
}

bootstrap();
```

## Middleware Patterns

### Error Handler

```typescript
// middleware/error-handler.ts
import { Request, Response, NextFunction } from 'express';
import { logger } from '../utils/logger';
import { AppError } from '../utils/errors';

export function errorHandler(
  err: Error,
  req: Request,
  res: Response,
  next: NextFunction
) {
  if (err instanceof AppError) {
    return res.status(err.statusCode).json({
      error: {
        code: err.code,
        message: err.message,
        ...(err.details && { details: err.details }),
      },
    });
  }

  logger.error('Unhandled error', {
    error: err.message,
    stack: err.stack,
    requestId: req.id,
  });

  return res.status(500).json({
    error: {
      code: 'INTERNAL_ERROR',
      message: 'An unexpected error occurred',
    },
  });
}
```

### Custom Errors

```typescript
// utils/errors.ts
export class AppError extends Error {
  constructor(
    public statusCode: number,
    public code: string,
    message: string,
    public details?: Record<string, unknown>
  ) {
    super(message);
    this.name = 'AppError';
  }
}

export class NotFoundError extends AppError {
  constructor(resource: string, id?: string) {
    super(404, 'NOT_FOUND', `${resource} not found${id ? `: ${id}` : ''}`);
  }
}

export class ValidationError extends AppError {
  constructor(message: string, details?: Record<string, unknown>) {
    super(400, 'VALIDATION_ERROR', message, details);
  }
}

export class UnauthorizedError extends AppError {
  constructor(message = 'Authentication required') {
    super(401, 'UNAUTHORIZED', message);
  }
}

export class ForbiddenError extends AppError {
  constructor(message = 'Insufficient permissions') {
    super(403, 'FORBIDDEN', message);
  }
}

export class ConflictError extends AppError {
  constructor(message: string) {
    super(409, 'CONFLICT', message);
  }
}

export class RateLimitError extends AppError {
  constructor(message = 'Too many requests') {
    super(429, 'RATE_LIMITED', message);
  }
}
```

### Rate Limiter

```typescript
// middleware/rate-limiter.ts
import rateLimit from 'express-rate-limit';
import RedisStore from 'rate-limit-redis';
import { redis } from '../config/redis';

export const rateLimiter = rateLimit({
  store: new RedisStore({
    sendCommand: (...args) => redis.call(...args),
  }),
  windowMs: 15 * 60 * 1000, // 15 minutes
  max: 100, // 100 requests per window
  standardHeaders: true,
  legacyHeaders: false,
  message: {
    error: {
      code: 'RATE_LIMITED',
      message: 'Too many requests, please try again later',
    },
  },
  keyGenerator: (req) => req.ip || req.headers['x-forwarded-for'] as string || 'unknown',
});

// Stricter limit for auth endpoints
export const authRateLimiter = rateLimit({
  store: new RedisStore({
    sendCommand: (...args) => redis.call(...args),
  }),
  windowMs: 15 * 60 * 1000,
  max: 5,
  message: {
    error: {
      code: 'RATE_LIMITED',
      message: 'Too many authentication attempts',
    },
  },
});
```

### Authentication Middleware

```typescript
// middleware/auth.ts
import { Request, Response, NextFunction } from 'express';
import jwt from 'jsonwebtoken';
import { UnauthorizedError } from '../utils/errors';
import { config } from '../config';

interface JwtPayload {
  sub: string;
  email: string;
  roles: string[];
}

declare global {
  namespace Express {
    interface Request {
      user?: JwtPayload;
    }
  }
}

export function authenticate(req: Request, res: Response, next: NextFunction) {
  const authHeader = req.headers.authorization;

  if (!authHeader?.startsWith('Bearer ')) {
    throw new UnauthorizedError('Missing or invalid authorization header');
  }

  const token = authHeader.split(' ')[1];

  try {
    const payload = jwt.verify(token, config.jwt.secret) as JwtPayload;
    req.user = payload;
    next();
  } catch (error) {
    if (error instanceof jwt.TokenExpiredError) {
      throw new UnauthorizedError('Token expired');
    }
    throw new UnauthorizedError('Invalid token');
  }
}

export function authorize(...roles: string[]) {
  return (req: Request, res: Response, next: NextFunction) => {
    if (!req.user) {
      throw new UnauthorizedError();
    }

    const hasRole = roles.some(role => req.user!.roles.includes(role));
    if (!hasRole) {
      throw new UnauthorizedError('Insufficient permissions');
    }

    next();
  };
}
```

## Validation with Zod

```typescript
// middleware/validate.ts
import { Request, Response, NextFunction } from 'express';
import { ZodSchema, ZodError } from 'zod';

export function validate(schema: ZodSchema) {
  return (req: Request, res: Response, next: NextFunction) => {
    try {
      req.body = schema.parse(req.body);
      next();
    } catch (error) {
      if (error instanceof ZodError) {
        return res.status(400).json({
          error: {
            code: 'VALIDATION_ERROR',
            message: 'Invalid request data',
            details: error.errors.map(e => ({
              field: e.path.join('.'),
              message: e.message,
            })),
          },
        });
      }
      next(error);
    }
  };
}

// Example schemas
import { z } from 'zod';

export const CreateUserSchema = z.object({
  email: z.string().email('Invalid email format'),
  name: z.string().min(2).max(100),
  password: z.string().min(8).regex(
    /^(?=.*[a-z])(?=.*[A-Z])(?=.*\d)/,
    'Password must contain uppercase, lowercase, and number'
  ),
});

export const UpdateUserSchema = CreateUserSchema.partial();

export const PaginationSchema = z.object({
  page: z.coerce.number().int().min(1).default(1),
  limit: z.coerce.number().int().min(1).max(100).default(20),
  sort: z.enum(['asc', 'desc']).default('desc'),
});
```

## Route Design

```typescript
// modules/users/routes.ts
import { Router } from 'express';
import { UserController } from './controller';
import { authenticate, authorize } from '../../middleware/auth';
import { validate } from '../../middleware/validate';
import { CreateUserSchema, UpdateUserSchema, PaginationSchema } from './model';

const router = Router();
const controller = new UserController();

router.get('/', authenticate, validate(PaginationSchema), controller.list);
router.get('/:id', authenticate, controller.getById);
router.post('/', authenticate, authorize('admin'), validate(CreateUserSchema), controller.create);
router.patch('/:id', authenticate, validate(UpdateUserSchema), controller.update);
router.delete('/:id', authenticate, authorize('admin'), controller.delete);

export { router as usersRouter };
```

## WebSocket with Socket.io

```typescript
// websocket/server.ts
import { Server } from 'socket.io';
import { createAdapter } from '@socket.io/redis-adapter';
import { createClient } from 'redis';
import jwt from 'jsonwebtoken';
import { config } from '../config';
import { logger } from '../utils/logger';

export function setupWebSocket(httpServer: any) {
  const io = new Server(httpServer, {
    cors: {
      origin: config.corsOrigin,
      credentials: true,
    },
    pingTimeout: 60000,
    pingInterval: 25000,
  });

  // Redis adapter for horizontal scaling
  const pubClient = createClient({ url: config.redis.url });
  const subClient = pubClient.duplicate();
  Promise.all([pubClient.connect(), subClient.connect()]).then(() => {
    io.adapter(createAdapter(pubClient, subClient));
  });

  // Authentication middleware
  io.use((socket, next) => {
    const token = socket.handshake.auth.token;
    if (!token) {
      return next(new Error('Authentication required'));
    }

    try {
      const payload = jwt.verify(token, config.jwt.secret);
      socket.data.user = payload;
      next();
    } catch {
      next(new Error('Invalid token'));
    }
  });

  io.on('connection', (socket) => {
    logger.info('Client connected', { userId: socket.data.user.sub });

    // Join user-specific room
    socket.join(`user:${socket.data.user.sub}`);

    socket.on('subscribe:project', (projectId: string) => {
      socket.join(`project:${projectId}`);
    });

    socket.on('unsubscribe:project', (projectId: string) => {
      socket.leave(`project:${projectId}`);
    });

    socket.on('disconnect', (reason) => {
      logger.info('Client disconnected', { userId: socket.data.user.sub, reason });
    });
  });

  return io;
}

// Usage in other parts of the app
export function broadcastToProject(io: Server, projectId: string, event: string, data: any) {
  io.to(`project:${projectId}`).emit(event, data);
}
```

## Background Jobs with BullMQ

```typescript
// jobs/queue.ts
import { Queue, Worker, QueueScheduler } from 'bullmq';
import { redis } from '../config/redis';
import { logger } from '../utils/logger';

const connection = {
  host: redis.options.host,
  port: redis.options.port,
};

// Queue definitions
export const emailQueue = new Queue('email', { connection });
export const reportQueue = new Queue('reports', { connection });

// Queue schedulers
new QueueScheduler('email', { connection });
new QueueScheduler('reports', { connection });

// Job processors
const emailWorker = new Worker('email', async (job) => {
  logger.info('Processing email job', { jobId: job.id, type: job.name });

  switch (job.name) {
    case 'welcome':
      await sendWelcomeEmail(job.data);
      break;
    case 'password-reset':
      await sendPasswordResetEmail(job.data);
      break;
    default:
      throw new Error(`Unknown job type: ${job.name}`);
  }
}, { connection, concurrency: 5 });

emailWorker.on('completed', (job) => {
  logger.info('Email job completed', { jobId: job.id });
});

emailWorker.on('failed', (job, error) => {
  logger.error('Email job failed', { jobId: job?.id, error: error.message });
});

// Enqueue jobs
export async function sendWelcomeEmail(data: { email: string; name: string }) {
  return emailQueue.add('welcome', data, {
    attempts: 3,
    backoff: { type: 'exponential', delay: 1000 },
    removeOnComplete: true,
    removeOnFail: false,
  });
}
```

## Logging with Pino

```typescript
// utils/logger.ts
import pino from 'pino';

export const logger = pino({
  level: process.env.LOG_LEVEL || 'info',
  formatters: {
    level: (label) => ({ level: label }),
  },
  timestamp: pino.stdTimeFunctions.isoTime,
  serializers: {
    err: pino.stdSerializers.err,
    req: pino.stdSerializers.req,
    res: pino.stdSerializers.res,
  },
  transport: process.env.NODE_ENV !== 'production'
    ? { target: 'pino-pretty', options: { colorize: true } }
    : undefined,
});

// Request logging middleware
export function requestLogger(req: any, res: any, next: any) {
  const start = Date.now();

  res.on('finish', () => {
    const duration = Date.now() - start;
    logger.info({
      requestId: req.id,
      method: req.method,
      url: req.originalUrl,
      statusCode: res.statusCode,
      duration,
      userAgent: req.headers['user-agent'],
    });
  });

  next();
}
```

## Environment Configuration

```typescript
// config/index.ts
import { z } from 'zod';

const envSchema = z.object({
  NODE_ENV: z.enum(['development', 'staging', 'production']).default('development'),
  PORT: z.coerce.number().default(3000),
  DATABASE_URL: z.string().url(),
  REDIS_URL: z.string().url(),
  JWT_SECRET: z.string().min(32),
  JWT_EXPIRES_IN: z.string().default('15m'),
  JWT_REFRESH_EXPIRES_IN: z.string().default('7d'),
  CORS_ORIGIN: z.string().optional(),
  LOG_LEVEL: z.enum(['fatal', 'error', 'warn', 'info', 'debug']).default('info'),
});

export const config = envSchema.parse(process.env);
```

## Cluster Mode

```typescript
// cluster.ts
import cluster from 'cluster';
import os from 'os';
import { createApp } from './app';
import { logger } from './utils/logger';

if (cluster.isPrimary) {
  const numCPUs = os.cpus().length;
  logger.info(`Master ${process.pid} starting ${numCPUs} workers`);

  for (let i = 0; i < numCPUs; i++) {
    cluster.fork();
  }

  cluster.on('exit', (worker, code, signal) => {
    logger.error(`Worker ${worker.process.pid} died (${code || signal})`);
    cluster.fork(); // Restart
  });
} else {
  const app = createApp();
  app.listen(3000, () => {
    logger.info(`Worker ${process.pid} started`);
  });
}
```

## Health Checks

```typescript
// modules/health/controller.ts
import { Request, Response } from 'express';
import { redis } from '../../config/redis';
import { prisma } from '../../config/database';

export async function healthCheck(req: Request, res: Response) {
  const checks: Record<string, { status: string; latency?: number }> = {};

  // Database check
  try {
    const start = Date.now();
    await prisma.$queryRaw`SELECT 1`;
    checks.database = { status: 'ok', latency: Date.now() - start };
  } catch {
    checks.database = { status: 'error' };
  }

  // Redis check
  try {
    const start = Date.now();
    await redis.ping();
    checks.redis = { status: 'ok', latency: Date.now() - start };
  } catch {
    checks.redis = { status: 'error' };
  }

  const allHealthy = Object.values(checks).every(c => c.status === 'ok');

  res.status(allHealthy ? 200 : 503).json({
    status: allHealthy ? 'ok' : 'degraded',
    timestamp: new Date().toISOString(),
    uptime: process.uptime(),
    checks,
  });
}
```

## API Versioning

```typescript
// modules/routes.ts
import { Router } from 'express';
import { usersRouter as usersRouterV1 } from './v1/users/routes';
import { usersRouter as usersRouterV2 } from './v2/users/routes';

const router = Router();

router.use('/v1/users', usersRouterV1);
router.use('/v2/users', usersRouterV2);

export { router as routes };
```

## Checklist

- [ ] Project structure follows conventions
- [ ] Error handling middleware in place
- [ ] Request validation on all endpoints
- [ ] Authentication middleware working
- [ ] Rate limiting configured
- [ ] CORS properly configured
- [ ] Structured logging with request IDs
- [ ] Environment config validated with Zod
- [ ] Graceful shutdown handling SIGTERM
- [ ] Health check endpoint returning status
- [ ] Background job queue configured
- [ ] WebSocket authentication working
- [ ] Cluster mode enabled for production
- [ ] Compression enabled for responses
