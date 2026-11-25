export const taskNumber = (task) => Number(task.slice(1));
export const planNumber = (plan) => Number(plan.slice(0, 3));

export const compareText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
export const compareTasks = (a, b) => taskNumber(a) - taskNumber(b);
export const comparePlans = (a, b) => planNumber(a) - planNumber(b) || compareText(a, b);
export const compareBySlug = (a, b) => compareText(a.slug, b.slug);
