Feature: Purpose Types
  The service provides purpose types that define which MCCs are allowed.

  Scenario: List all purpose types
    When I list all purpose types
    Then I should see at least 4 purpose types

  Scenario: Get a specific purpose type
    When I get the "health" purpose type
    Then the purpose code should be "health"
    And it should have allowed MCCs
