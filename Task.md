# Task List

## Bug: Terminals Overlapping (#260)

**Status:** Open  
**Labels:** bug  
**Assignee:** lassejlv  
**Reported by:** janburzinski  
**Version:** 0.1.59

### Description
After closing split screen sessions, terminals sometimes overlap (two terminals stacked on top of each other).

### Image
![Terminal Overlap Issue](https://github.com/user-attachments/assets/60d73b62-c4ff-4f37-9210-d96d45ddfd18)

### Steps to Reproduce
1. Split tabs
2. Delete them
3. Issue doesn't happen every time (intermittent)

### Expected Behavior
Tabs should close cleanly without overlapping

### Environment
- OS: macOS
- Version: 0.1.59

### Reference
https://github.com/lassejlv/termy/issues/260

---

## Pending Tasks

- [ ] Investigate terminal overlap issue when closing split tabs
- [ ] Find root cause of intermittent behavior
- [ ] Implement fix for clean tab closure
