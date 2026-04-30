---
name: sap-cap-ui5-dev
description: |
  SAP CAP (Node.js/TypeScript) + SAPUI5/Fiori development skill for developer agents.
  Covers CDS modeling, OData V4 services, custom handlers, UI5 views/controllers,
  Fiori Elements annotations, testing patterns, and project structure conventions.
  Use when implementing features in SAP CAP + UI5 projects.
---

# SAP CAP + UI5 Developer Guide

You are implementing code in an SAP CAP (Cloud Application Programming) + SAPUI5 project.
Follow these patterns strictly. Always use the project's existing conventions when they
differ from the defaults shown here.

---

## 1. Project Detection & Setup

### Detect SAP CAP Project

```bash
# CAP project indicators (check in order):
# 1. .cdsrc.json exists
# 2. package.json contains @sap/cds in dependencies
# 3. db/ + srv/ + app/ directory structure
cat package.json | grep -q "@sap/cds" && echo "CAP project detected"
```

### Standard CAP Project Structure

```
project-root/
├── package.json           # Dependencies, scripts, cds config
├── .cdsrc.json            # CDS configuration
├── db/
│   ├── schema.cds         # Data model definitions
│   └── data/              # CSV seed data (entity-name.csv)
├── srv/
│   ├── service.cds        # Service definitions
│   ├── service.js/.ts     # Custom handlers
│   └── annotations.cds    # UI annotations
├── app/
│   └── <app-name>/
│       └── webapp/
│           ├── manifest.json
│           ├── Component.js/.ts
│           ├── view/
│           ├── controller/
│           ├── model/
│           ├── i18n/
│           └── css/
└── test/
    └── *.test.js/.ts
```

### Key Commands

```bash
# ALWAYS read package.json scripts first
cat package.json | jq '.scripts'

# Common commands
cds watch              # Dev server with hot reload
cds build              # Production build
cds deploy --to hana   # Deploy to HANA
npm run lint           # ESLint
npm test               # Run tests
```

---

## 2. CDS Data Modeling

### Entity Definitions

```cds
namespace com.sap.sfm;

using { cuid, managed, temporal } from '@sap/cds/common';
using { Currency, Country } from '@sap/cds/common';

// Base entity with common fields
entity Products : cuid, managed {
  name        : String(100) @mandatory;
  description : localized String(1000);
  price       : Decimal(12,3);
  currency    : Currency;
  category    : Association to Categories;
  items       : Composition of many ProductItems on items.product = $self;
  status      : Status default 'draft';
}

// Enum-like type
type Status : String enum {
  draft;
  active;
  inactive;
  deleted;
}

// Code list / value help entity
@cds.autoexpose
entity Categories : cuid {
  name     : localized String(50);
  code     : String(10) @mandatory;
  products : Association to many Products on products.category = $self;
}

// Child entity (composition)
entity ProductItems : cuid, managed {
  product     : Association to Products;
  description : String(200);
  quantity    : Integer default 1;
  unit        : String(3);
}
```

### Key CDS Patterns

| Pattern | Usage |
|---------|-------|
| `cuid` | Auto-generated UUID key |
| `managed` | createdAt, createdBy, modifiedAt, modifiedBy |
| `temporal` | validFrom, validTo |
| `localized` | Multi-language text support |
| `Composition` | Parent owns children (cascade delete) |
| `Association` | Reference relationship (no cascade) |
| `@mandatory` | Not-null constraint |
| `@readonly` | Immutable after creation |
| `@assert.range` | Value range validation |

### Aspects & Reuse

```cds
// Define reusable aspect
aspect Auditable {
  auditLog : Composition of many AuditEntries on auditLog.parent = $self;
}

// Apply aspect
entity Orders : cuid, managed, Auditable {
  // ...
}

// Import from other CDS modules
using { com.sap.sfm.reuse as reuse } from '@c21/sfm-reuse-cds-models';
```

---

## 3. Service Definitions

### Service Layer

```cds
using { com.sap.sfm as db } from '../db/schema';

// Main application service
service NetworkCalculationService @(path: '/network-calculation') {

  @odata.draft.enabled    // Enable draft for complex forms
  entity Calculations as projection on db.Calculations {
    *,
    network.name as networkName : String
  } excluding { deletedAt } actions {
    // Bound actions (on single entity)
    action submit() returns Calculations;
    action approve() returns Calculations;
    action reject(reason: String) returns Calculations;
  };

  @readonly
  entity Networks as projection on db.Networks;

  // Unbound actions/functions
  action   runCalculation(networkId: UUID) returns CalculationResult;
  function getStatus(calculationId: UUID) returns StatusInfo;

  // Value help entities
  @cds.odata.valuelist
  entity VH_Networks as projection on db.Networks { key ID, name, code };
}

// Admin service (separate authorization)
service AdminService @(path: '/admin', requires: 'admin') {
  entity Configurations as projection on db.Configurations;
}
```

### Key Service Patterns

- `@odata.draft.enabled` — Enables draft-based editing (for complex forms)
- `@readonly` — No CUD operations exposed
- `@restrict` — Authorization annotations
- `@cds.odata.valuelist` — Marks entity as value help source
- `actions {}` — Bound custom actions on entity
- `action/function` — Unbound operations at service level

---

## 4. Custom Handlers (TypeScript)

### Handler Structure

```typescript
import cds from '@sap/cds';
import { Request } from '@sap/cds';

export default class NetworkCalculationService extends cds.ApplicationService {

  async init() {
    const { Calculations, Networks } = this.entities;

    // --- Validation ---
    this.before(['CREATE', 'UPDATE'], Calculations, this.validateCalculation);

    // --- Business Logic ---
    this.on('submit', Calculations, this.onSubmit);
    this.on('approve', Calculations, this.onApprove);
    this.on('reject', Calculations, this.onReject);

    // --- Read extensions ---
    this.after('READ', Calculations, this.enrichCalculations);

    // --- Unbound actions ---
    this.on('runCalculation', this.onRunCalculation);

    await super.init();
  }

  private async validateCalculation(req: Request) {
    const { name, networkId } = req.data;

    if (!name?.trim()) {
      req.error(400, 'NAME_REQUIRED', 'name');
    }

    if (networkId) {
      const { Networks } = this.entities;
      const network = await SELECT.one.from(Networks).where({ ID: networkId });
      if (!network) {
        req.error(404, 'NETWORK_NOT_FOUND', 'networkId');
      }
    }
  }

  private async onSubmit(req: Request) {
    const { ID } = req.params[0] as { ID: string };
    const { Calculations } = this.entities;

    const calc = await SELECT.one.from(Calculations).where({ ID });
    if (calc.status !== 'draft') {
      return req.error(409, 'INVALID_STATE', undefined, [calc.status, 'draft']);
    }

    await UPDATE(Calculations).set({ status: 'submitted' }).where({ ID });
    return SELECT.one.from(Calculations).where({ ID });
  }

  private async onApprove(req: Request) {
    const { ID } = req.params[0] as { ID: string };
    const { Calculations } = this.entities;

    await UPDATE(Calculations)
      .set({ status: 'approved', approvedBy: req.user.id, approvedAt: new Date() })
      .where({ ID });
    return SELECT.one.from(Calculations).where({ ID });
  }

  private async onReject(req: Request) {
    const { ID } = req.params[0] as { ID: string };
    const { reason } = req.data;
    const { Calculations } = this.entities;

    await UPDATE(Calculations)
      .set({ status: 'rejected', rejectionReason: reason })
      .where({ ID });
    return SELECT.one.from(Calculations).where({ ID });
  }

  private enrichCalculations(results: any[], req: Request) {
    for (const calc of results) {
      // Add computed fields
      calc.statusCriticality = this.getStatusCriticality(calc.status);
    }
  }

  private getStatusCriticality(status: string): number {
    const map: Record<string, number> = {
      draft: 0,       // Neutral
      submitted: 2,   // Critical (pending)
      approved: 3,    // Positive
      rejected: 1,    // Negative
    };
    return map[status] ?? 0;
  }

  private async onRunCalculation(req: Request) {
    const { networkId } = req.data;
    // Business logic...
    return { status: 'completed', calculationId: '...' };
  }
}
```

### Handler Patterns

| Hook | Use Case |
|------|----------|
| `this.before('CREATE', Entity, fn)` | Validation, default values |
| `this.before('UPDATE', Entity, fn)` | Validation, guard state transitions |
| `this.before('DELETE', Entity, fn)` | Soft delete, cascade checks |
| `this.on('ACTION', Entity, fn)` | Custom action implementation |
| `this.after('READ', Entity, fn)` | Enrich response, computed fields |
| `this.on('READ', Entity, fn)` | Override default read (rare) |

### CDS Query Language (CQL)

```typescript
// SELECT
const items = await SELECT.from(Products).where({ status: 'active' });
const one = await SELECT.one.from(Products).where({ ID: id });
const count = await SELECT.one.from(Products).columns('count(*) as count');
const expanded = await SELECT.from(Products).columns('*', 'category { name }');

// INSERT
await INSERT.into(Products).entries({ name: 'New', status: 'draft' });

// UPDATE
await UPDATE(Products).set({ status: 'active' }).where({ ID: id });

// DELETE
await DELETE.from(Products).where({ ID: id });

// Subqueries
const results = await SELECT.from(Products)
  .where({ category_ID: { in: SELECT('ID').from(Categories).where({ active: true }) } });
```

### Error Handling

```typescript
// Field-level error (shows on specific form field)
req.error(400, 'INVALID_VALUE', 'fieldName');

// Entity-level error
req.error(409, 'Cannot transition from {0} to {1}', undefined, [currentState, targetState]);

// Warning (non-blocking)
req.warn(200, 'This action cannot be undone');

// Info
req.info(200, 'Calculation started in background');

// i18n messages (preferred)
req.error(400, 'msg_invalid_status', 'status'); // reads from i18n/messages.properties
```

---

## 5. UI Annotations (CDS)

### List Report Annotations

```cds
using NetworkCalculationService from './service';

annotate NetworkCalculationService.Calculations with @(
  UI: {
    // Header info for Object Page
    HeaderInfo: {
      TypeName: '{i18n>Calculation}',
      TypeNamePlural: '{i18n>Calculations}',
      Title: { Value: name },
      Description: { Value: networkName },
      ImageUrl: 'sap-icon://calculate'
    },

    // Filter bar fields
    SelectionFields: [ name, network_ID, status, createdAt ],

    // Table columns
    LineItem: [
      { Value: name, Label: '{i18n>name}' },
      { Value: networkName, Label: '{i18n>network}' },
      { Value: status, Label: '{i18n>status}', Criticality: statusCriticality },
      { Value: createdAt, Label: '{i18n>createdAt}' },
      { Value: modifiedAt, Label: '{i18n>modifiedAt}' },
      // Inline action
      { $Type: 'UI.DataFieldForAction', Action: 'NetworkCalculationService.submit', Label: '{i18n>submit}' }
    ],

    // Object Page sections
    Facets: [
      { $Type: 'UI.ReferenceFacet', Target: '@UI.FieldGroup#General', Label: '{i18n>general}' },
      { $Type: 'UI.ReferenceFacet', Target: '@UI.FieldGroup#Parameters', Label: '{i18n>parameters}' },
      { $Type: 'UI.ReferenceFacet', Target: 'items/@UI.LineItem', Label: '{i18n>items}' }
    ],

    // Field groups
    FieldGroup#General: {
      Data: [
        { Value: name },
        { Value: description },
        { Value: network_ID },
        { Value: status, Criticality: statusCriticality }
      ]
    },

    FieldGroup#Parameters: {
      Data: [
        { Value: startDate },
        { Value: endDate },
        { Value: scope }
      ]
    }
  }
);

// Value help for network field
annotate NetworkCalculationService.Calculations with {
  network @(
    Common: {
      Text: network.name,
      TextArrangement: #TextOnly,
      ValueList: {
        CollectionPath: 'VH_Networks',
        Parameters: [
          { $Type: 'Common.ValueListParameterInOut', LocalDataProperty: network_ID, ValueListProperty: 'ID' },
          { $Type: 'Common.ValueListParameterDisplayOnly', ValueListProperty: 'name' },
          { $Type: 'Common.ValueListParameterDisplayOnly', ValueListProperty: 'code' }
        ]
      }
    }
  );
};
```

### Key Annotation Patterns

| Annotation | Purpose |
|------------|---------|
| `@UI.HeaderInfo` | Object page header (title, description, icon) |
| `@UI.SelectionFields` | Filter bar fields in List Report |
| `@UI.LineItem` | Table columns |
| `@UI.Facets` | Object page sections/tabs |
| `@UI.FieldGroup` | Form field grouping |
| `@UI.DataPoint` | KPI/header numeric values |
| `@UI.Chart` | Analytical charts |
| `@UI.Identification` | Object page header actions |
| `@Common.ValueList` | Dropdown/value help |
| `@Common.Text` | Display text for code fields |
| `@UI.Criticality` | Semantic coloring (0-5) |
| `@UI.Hidden` | Hide field from UI |

### Criticality Values

| Value | Meaning | Color |
|-------|---------|-------|
| 0 | Neutral | Grey |
| 1 | Negative | Red |
| 2 | Critical | Orange |
| 3 | Positive | Green |
| 5 | Information | Blue |

---

## 6. UI5 Views (XML)

### Naming Conventions

- Views: `PascalCase.view.xml` (e.g., `NetworkList.view.xml`)
- Controllers: `PascalCase.controller.ts` (e.g., `NetworkList.controller.ts`)
- Fragments: `PascalCase.fragment.xml`
- Models: `camelCase.ts`

### Standard View Template

```xml
<mvc:View
  controllerName="com.sap.sfm.network.controller.NetworkList"
  xmlns="sap.m"
  xmlns:mvc="sap.ui.core.mvc"
  xmlns:f="sap.f"
  xmlns:core="sap.ui.core"
  xmlns:semantic="sap.f.semantic">

  <semantic:SemanticPage
    id="networkListPage"
    headerPinnable="false"
    toggleHeaderOnTitleClick="false">

    <semantic:titleHeading>
      <Title text="{i18n>networkListTitle}" level="H2"/>
    </semantic:titleHeading>

    <semantic:content>
      <!-- Table content here -->
    </semantic:content>

    <semantic:titleMainAction>
      <semantic:TitleMainAction text="{i18n>create}" press=".onCreate"/>
    </semantic:titleMainAction>

  </semantic:SemanticPage>
</mvc:View>
```

### Common Control Patterns

```xml
<!-- Smart Table (Fiori Elements style) -->
<smartTable:SmartTable
  id="networkTable"
  entitySet="Networks"
  smartFilterId="networkFilter"
  tableType="ResponsiveTable"
  useVariantManagement="true"
  useExportToExcel="true"
  enableAutoBinding="true"
  header="{i18n>networks}"
  showRowCount="true"
  demandPopin="true">
  <smartTable:customToolbar>
    <OverflowToolbar design="Transparent">
      <ToolbarSpacer/>
      <Button icon="sap-icon://action" press=".onAction"/>
    </OverflowToolbar>
  </smartTable:customToolbar>
</smartTable:SmartTable>

<!-- OData V4 Table binding -->
<Table
  id="itemsTable"
  items="{
    path: '/Calculations',
    parameters: {
      $count: true,
      $orderby: 'createdAt desc',
      $expand: 'network'
    }
  }"
  growing="true"
  growingThreshold="30"
  sticky="HeaderToolbar,ColumnHeaders">
```

---

## 7. UI5 Controllers (TypeScript)

### Controller Template

```typescript
import Controller from "sap/ui/core/mvc/Controller";
import MessageBox from "sap/m/MessageBox";
import MessageToast from "sap/m/MessageToast";
import Filter from "sap/ui/model/Filter";
import FilterOperator from "sap/ui/model/FilterOperator";
import ODataListBinding from "sap/ui/model/odata/v4/ODataListBinding";
import JSONModel from "sap/ui/model/json/JSONModel";
import { Route$PatternMatchedEvent } from "sap/ui/core/routing/Route";

/**
 * @namespace com.sap.sfm.network.controller
 */
export default class NetworkList extends Controller {

  public onInit(): void {
    const oViewModel = new JSONModel({
      busy: false,
      itemCount: 0
    });
    this.getView()!.setModel(oViewModel, "view");
  }

  public onSearch(oEvent: any): void {
    const sQuery = oEvent.getParameter("query");
    const oTable = this.byId("networkTable") as any;
    const oBinding = oTable.getBinding("items") as ODataListBinding;

    const aFilters: Filter[] = [];
    if (sQuery) {
      aFilters.push(new Filter({
        filters: [
          new Filter("name", FilterOperator.Contains, sQuery),
          new Filter("code", FilterOperator.Contains, sQuery)
        ],
        and: false
      }));
    }

    oBinding.filter(aFilters);
  }

  public onCreate(): void {
    (this.getOwnerComponent() as any).getRouter().navTo("create");
  }

  public onItemPress(oEvent: any): void {
    const oSource = oEvent.getSource();
    const oContext = oSource.getBindingContext();
    const sID = oContext.getProperty("ID");

    (this.getOwnerComponent() as any).getRouter().navTo("detail", {
      id: sID
    });
  }

  private getText(sKey: string, aArgs?: string[]): string {
    return (this.getOwnerComponent() as any)
      .getModel("i18n").getResourceBundle().getText(sKey, aArgs);
  }
}
```

### OData V4 Operations in Controllers

```typescript
// Create
const oModel = this.getView()!.getModel() as any;
const oListBinding = oModel.bindList("/Calculations");
const oContext = oListBinding.create({
  name: "New Calculation",
  status: "draft"
});
await oContext.created(); // Wait for server response

// Update (via binding context)
const oContext = oEvent.getSource().getBindingContext();
oContext.setProperty("status", "active");
await oModel.submitBatch("updateGroup");

// Delete
const oContext = oEvent.getSource().getBindingContext();
oContext.delete();

// Bound action
const oContext = this.getView()!.getBindingContext();
const oOperation = oContext.getModel().bindContext(
  `${oContext.getPath()}/NetworkCalculationService.submit(...)`
);
await oOperation.invoke();
await oContext.refresh(); // Refresh to get updated data

// Unbound action
const oModel = this.getView()!.getModel() as any;
const oOperation = oModel.bindContext("/runCalculation(...)");
oOperation.setParameter("networkId", sNetworkId);
await oOperation.invoke();
const oResult = oOperation.getBoundContext().getObject();
```

---

## 8. Fiori Elements (List Report + Object Page)

### manifest.json for Fiori Elements

```json
{
  "sap.app": {
    "id": "com.sap.sfm.network.calculations",
    "type": "application",
    "dataSources": {
      "mainService": {
        "uri": "/network-calculation/",
        "type": "OData",
        "settings": { "odataVersion": "4.0" }
      }
    }
  },
  "sap.ui5": {
    "dependencies": {
      "libs": { "sap.fe.templates": {} }
    },
    "models": {
      "": {
        "dataSource": "mainService",
        "settings": {
          "operationMode": "Server",
          "autoExpandSelect": true,
          "earlyRequests": true
        }
      }
    },
    "routing": {
      "routes": [
        { "pattern": ":?query:", "name": "CalculationsList", "target": "CalculationsList" },
        { "pattern": "Calculations({key}):?query:", "name": "CalculationsDetail", "target": "CalculationsDetail" }
      ],
      "targets": {
        "CalculationsList": {
          "type": "Component",
          "id": "CalculationsList",
          "name": "sap.fe.templates.ListReport",
          "options": {
            "settings": {
              "entitySet": "Calculations",
              "variantManagement": "Page",
              "navigation": {
                "Calculations": { "detail": { "route": "CalculationsDetail" } }
              }
            }
          }
        },
        "CalculationsDetail": {
          "type": "Component",
          "id": "CalculationsDetail",
          "name": "sap.fe.templates.ObjectPage",
          "options": {
            "settings": {
              "entitySet": "Calculations",
              "editableHeaderContent": false
            }
          }
        }
      }
    }
  }
}
```

---

## 9. Testing Patterns

### Unit Tests (CAP Service)

```typescript
import cds from '@sap/cds';
const { expect } = cds.test(__dirname + '/..');

describe('NetworkCalculationService', () => {

  it('should create a calculation', async () => {
    const { Calculations } = cds.entities('NetworkCalculationService');
    const result = await INSERT.into(Calculations).entries({
      name: 'Test Calc',
      status: 'draft'
    });
    expect(result).to.exist;
  });

  it('should reject invalid status transition', async () => {
    const srv = await cds.connect.to('NetworkCalculationService');
    try {
      await srv.send('submit', { ID: 'already-submitted-id' });
      expect.fail('Should have thrown');
    } catch (e: any) {
      expect(e.code).to.equal(409);
    }
  });

  it('should validate required fields', async () => {
    const srv = await cds.connect.to('NetworkCalculationService');
    try {
      await srv.send('CREATE', 'Calculations', { name: '' });
      expect.fail('Should have thrown');
    } catch (e: any) {
      expect(e.code).to.equal(400);
    }
  });
});
```

### Integration Tests (OData)

```typescript
import cds from '@sap/cds';
const { GET, POST, PATCH, DELETE, expect } = cds.test(__dirname + '/..');

describe('OData Integration', () => {

  it('GET /network-calculation/Calculations', async () => {
    const { status, data } = await GET('/network-calculation/Calculations');
    expect(status).to.equal(200);
    expect(data.value).to.be.an('array');
  });

  it('POST /network-calculation/Calculations', async () => {
    const { status, data } = await POST('/network-calculation/Calculations', {
      name: 'Integration Test',
      status: 'draft'
    });
    expect(status).to.equal(201);
    expect(data.ID).to.exist;
  });

  it('bound action: submit', async () => {
    // Create first
    const { data: created } = await POST('/network-calculation/Calculations', {
      name: 'Submit Test', status: 'draft'
    });
    // Submit
    const { status } = await POST(
      `/network-calculation/Calculations(${created.ID})/NetworkCalculationService.submit`
    );
    expect(status).to.equal(200);
  });
});
```

---

## 10. i18n Conventions

### File: `i18n/i18n.properties`

```properties
# App
appTitle=Network Calculations
appDescription=Manage network calculation scenarios

# List
networkListTitle=Network Calculations
create=Create
refresh=Refresh

# Table columns
colName=Name
colNetwork=Network
colStatus=Status
colCreatedAt=Created

# Status
statusDraft=Draft
statusSubmitted=Submitted
statusApproved=Approved
statusRejected=Rejected

# Actions
submit=Submit
approve=Approve
reject=Reject
save=Save
cancel=Cancel
edit=Edit
delete=Delete

# Messages
msgSaved=Changes saved successfully
msgDeleted=Item deleted
msgDeleteConfirm=Are you sure you want to delete this item?
msgInvalidStatus=Cannot transition from {0} to {1}
```

### File: `i18n/messages.properties` (CAP server-side)

```properties
msg_invalid_status=Invalid status transition: {0} -> {1}
msg_name_required=Name is required
msg_network_not_found=Network not found
```

---

## 11. MCP Tools Available

When developing in this stack, you have these MCP tools available:

| MCP Server | Use For |
|------------|---------|
| `cap-js-mcp` | CDS modeling questions, CAP API reference, service patterns |
| `ui5-mcp` | UI5 API reference, component lookup, linting |
| `fiori-tools` | Fiori Elements configuration, annotation help, guided development |
| `figma` | Reading Figma designs for UI implementation reference |

**Always use MCP tools** to verify API details before writing code. Do NOT guess
control properties or CDS syntax — look them up.

---

## 12. Code Quality Rules

### TypeScript (CAP handlers)

- Use strict TypeScript (`strict: true` in tsconfig)
- Type all function parameters and return types
- Use `import cds from '@sap/cds'` (not require)
- Use async/await over promise chains
- Handle errors with `req.error()` / `req.reject()` — never throw raw errors

### UI5 Views (XML)

- Always use i18n for user-visible text: `{i18n>key}`
- Use data binding over hardcoded values
- Prefer semantic controls (`SemanticPage`, `DynamicPage`)
- Set `id` on controls you reference in controller
- Use fragments for reusable UI pieces

### UI5 Controllers (TypeScript)

- One controller per view (1:1 mapping)
- Use `this.byId()` to access controls (never `sap.ui.getCore().byId()`)
- Use `this.getOwnerComponent()` for model access
- Clean up event handlers in `onExit()`
- Use formatters for display logic (not controller methods)

### General

- Never hardcode URLs — use manifest.json dataSources
- Never store credentials in code
- Always externalize text (i18n)
- Use OData batch for multiple operations
- Prefer Fiori Elements + annotations over freestyle when possible
