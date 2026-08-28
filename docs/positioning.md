# Positioning

## Category

**Spacecraft Interaction Authorization**

This project provides authorization infrastructure for autonomous physical
interactions between spacecraft, stations, servicing vehicles, and robots
operated by different organizations.

## Component terminology

Until durable project and component brands are selected, documentation uses
neutral names that describe responsibility:

- **Spacecraft Interaction Authorization:** the system and product category
- **interaction authorization protocol:** normative peer messages and behavior
- **authorization core:** portable policy, state, and entitlement implementation
- **integration adapter:** binding to a flight, robotics, or transport framework
- **conformance suite:** implementation-independent vectors and compatibility tests
- **operational profile:** constraints for a specific interaction class
- **reference environment:** executable evidence for representative operations
- **docking reference environment:** the ROS 2/Gazebo demonstration in this repository

These terms are descriptions, not product brands. Existing source package names
remain implementation identifiers until a deliberate compatibility migration.

## One-sentence description

An open, fail-closed authorization layer that lets independently operated space
systems establish trust, grant narrowly scoped authority, and approve individual
proximity actions without depending on continuous ground control.

## Problem statement

Secure communications can establish that a message was protected in transit.
They do not, by themselves, determine whether a particular vehicle may perform a
particular safety-critical action at a particular place and time.

As spacecraft become more autonomous and commercial infrastructure serves
vehicles from multiple operators, missions need interoperable answers to:

- Who is requesting access or motion?
- Which operator, vehicle, and encounter session does that identity represent?
- What action is permitted, at which port or resource, and for how long?
- Which local safety evidence must also be satisfied?
- Has the authority expired, been revoked, or already been consumed?
- What safe state applies when evidence or communication is lost?
- What audit evidence explains the resulting decision?

## Product outcome

The system converts verified identity, operator policy, encounter state, and
local safety evidence into an auditable decision. An allowed decision produces a
short-lived, narrowly scoped, replay-resistant entitlement consumed at the
protected state-transition or actuation boundary. Every other outcome fails
closed according to mission policy.

## Initial use case

The first reference use case is visiting-vehicle admission through rendezvous,
proximity operations, and docking stages. Docking makes the authorization model
concrete because each transition has physical consequences and independent
parties may control the station and vehicle.

The simulation exists to build operational credibility for this use case. It
should demonstrate the capability in realistic workflows and adverse conditions,
not evolve into a separate simulation product.

The same model is intended to extend to:

- on-orbit servicing and inspection
- refueling and resource transfer
- capture, berthing, and towing
- in-space assembly and manufacturing
- shared lunar or orbital infrastructure
- coordinated robotic operations

## What this project is not

- It is not a communications encryption replacement.
- It is not a guidance, navigation, or control system.
- It is not a mechanical docking standard.
- It is not a spacecraft flight-software framework.
- It is not currently flight-qualified.
- The ROS 2/Gazebo application is not the deployable product.

The project composes with secure communications, flight frameworks, docking
standards, and mission safety logic. It supplies the missing application-level
authorization contract between them.

## Intended adopters

- commercial station and infrastructure operators
- visiting-vehicle and logistics providers
- on-orbit servicing and refueling companies
- spacecraft primes and flight-software integrators
- civil and defense organizations coordinating multi-operator missions
- standards, assurance, and research organizations

## Adoption promise

An adopter should be able to use the protocol and portable kernel without ROS 2,
Gazebo, Python, this dashboard, or a commercial service agreement. Integration
services, mission policy engineering, qualification evidence, certified builds,
and long-term support may be offered around that open foundation.
